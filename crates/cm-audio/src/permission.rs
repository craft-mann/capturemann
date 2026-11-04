//! Microphone permission state. `check` never prompts; the prompt only happens in
//! [`request_microphone_access`], which the app calls from an explicit user action (setup panel
//! "Allow microphone").

use std::time::Duration;

use cm_core::{CmError, Permission};
use serde::{Deserialize, Serialize};

/// OS authorization state for a protected device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    /// The user has not been asked yet; the next capture attempt would show the system prompt.
    NotDetermined,
    Denied,
    /// Blocked by policy (MDM, parental controls).
    Restricted,
    Authorized,
}

impl PermissionState {
    pub fn is_authorized(self) -> bool {
        self == PermissionState::Authorized
    }

    /// The message shown in the setup panel (AUD-1.AC5).
    pub fn message(self) -> Option<&'static str> {
        match self {
            PermissionState::Authorized => None,
            PermissionState::NotDetermined if cfg!(windows) => Some(
                "CaptureMann needs microphone access. Turn it on in Settings > Privacy & security > Microphone.",
            ),
            PermissionState::NotDetermined => {
                Some("CaptureMann needs microphone access. Choose Allow when macOS asks.")
            }
            PermissionState::Denied if cfg!(windows) => Some(
                "Microphone access is off. Turn it on in Settings > Privacy & security > Microphone, including Let desktop apps access your microphone.",
            ),
            PermissionState::Denied => Some(
                "Microphone access is off. Turn it on in System Settings > Privacy & Security > Microphone.",
            ),
            PermissionState::Restricted => {
                Some("Microphone access is restricted by this device's policy.")
            }
        }
    }
}

/// The state Windows' microphone privacy switches add up to: a group policy that denies apps
/// (`2`) is `Restricted`; `Deny` in any of the device, user or desktop-apps switches is `Denied`;
/// a policy that forces access on (`1`) or no switch at all is `Authorized`. Values are the
/// registry strings `Allow` and `Deny`, compared without regard to case.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn windows_microphone_state(
    policy: Option<u32>,
    machine: Option<&str>,
    user: Option<&str>,
    desktop_apps: Option<&str>,
) -> PermissionState {
    match policy {
        Some(2) => return PermissionState::Restricted,
        Some(1) => return PermissionState::Authorized,
        _ => {}
    }
    let denied = |v: Option<&str>| v.is_some_and(|v| v.trim().eq_ignore_ascii_case("deny"));
    if denied(machine) || denied(user) || denied(desktop_apps) {
        PermissionState::Denied
    } else {
        PermissionState::Authorized
    }
}

/// Current microphone authorization. Never prompts, safe to call from tests and pollers.
/// On Windows it reads the privacy switches of Settings > Privacy & security > Microphone (the
/// device, the user and desktop apps) and the group policy; other systems report `Authorized`.
pub fn microphone_permission() -> PermissionState {
    #[cfg(target_os = "macos")]
    {
        crate::sys::mac_permission::microphone_status()
    }
    #[cfg(windows)]
    {
        crate::sys::win_consent::microphone_state()
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        PermissionState::Authorized
    }
}

/// Asks the OS for microphone access, showing the system prompt if the state is
/// `NotDetermined`, and waits for the answer (up to `timeout`). Returns the resulting state.
/// Call only in response to a user action.
pub fn request_microphone_access(timeout: Duration) -> PermissionState {
    #[cfg(target_os = "macos")]
    {
        crate::sys::mac_permission::request_microphone(timeout)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = timeout;
        PermissionState::Authorized
    }
}

/// `Err(permission_missing(microphone))` unless the microphone may be opened without a prompt.
pub fn require_microphone() -> Result<(), CmError> {
    let state = microphone_permission();
    match state.message() {
        None => Ok(()),
        Some(msg) => Err(CmError::permission_missing(Permission::Microphone, msg)),
    }
}

/// Screen Recording access, which ScreenCaptureKit needs for system audio. Never prompts.
pub fn screen_recording_permission() -> PermissionState {
    #[cfg(target_os = "macos")]
    {
        if crate::sys::mac_permission::screen_recording_granted() {
            PermissionState::Authorized
        } else {
            // CoreGraphics cannot tell "never asked" from "denied" without prompting.
            PermissionState::NotDetermined
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionState::Authorized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_does_not_prompt_and_returns_a_state() {
        // Reading the state is safe in CI and on a developer machine: it never shows UI.
        let s = microphone_permission();
        assert!(matches!(
            s,
            PermissionState::NotDetermined
                | PermissionState::Denied
                | PermissionState::Restricted
                | PermissionState::Authorized
        ));
    }

    #[test]
    fn messages_exist_for_every_blocked_state() {
        assert!(PermissionState::Authorized.message().is_none());
        for s in [
            PermissionState::NotDetermined,
            PermissionState::Denied,
            PermissionState::Restricted,
        ] {
            assert!(s.message().is_some());
            assert!(!s.is_authorized());
        }
    }

    #[test]
    fn windows_switches_add_up_to_one_state() {
        let st = windows_microphone_state;
        assert_eq!(st(None, None, None, None), PermissionState::Authorized);
        assert_eq!(
            st(None, Some("Allow"), Some("Allow"), Some("Allow")),
            PermissionState::Authorized
        );
        assert_eq!(
            st(None, Some("Allow"), Some("Deny"), Some("Allow")),
            PermissionState::Denied
        );
        assert_eq!(
            st(None, Some("Allow"), Some("Allow"), Some("deny")),
            PermissionState::Denied
        );
        assert_eq!(st(None, Some("Deny"), None, None), PermissionState::Denied);
        assert_eq!(
            st(Some(2), Some("Allow"), None, None),
            PermissionState::Restricted
        );
        assert_eq!(
            st(Some(1), Some("Deny"), Some("Deny"), None),
            PermissionState::Authorized
        );
        assert_eq!(
            st(Some(0), None, Some("Deny"), None),
            PermissionState::Denied
        );
    }

    #[test]
    fn serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&PermissionState::NotDetermined).unwrap(),
            "\"not_determined\""
        );
    }
}
