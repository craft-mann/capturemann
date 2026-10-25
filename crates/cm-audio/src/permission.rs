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
            PermissionState::NotDetermined => {
                Some("CaptureMann needs microphone access. Choose Allow when macOS asks.")
            }
            PermissionState::Denied => Some(
                "Microphone access is off. Turn it on in System Settings > Privacy & Security > Microphone.",
            ),
            PermissionState::Restricted => {
                Some("Microphone access is restricted by this device's policy.")
            }
        }
    }
}

/// Current microphone authorization. Never prompts, safe to call from tests and pollers.
/// On Windows the privacy switch is only visible as a stream error, so this reports `Authorized`.
pub fn microphone_permission() -> PermissionState {
    #[cfg(target_os = "macos")]
    {
        crate::sys::mac_permission::microphone_status()
    }
    #[cfg(not(target_os = "macos"))]
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
    fn serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&PermissionState::NotDetermined).unwrap(),
            "\"not_determined\""
        );
    }
}
