//! Camera permission state. `check` never prompts; `request_camera_access` is the only prompt.

use cm_core::{CmError, Permission};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraPermission {
    /// Never asked. A recording would trigger the prompt (or be killed by macOS if the host
    /// process has no `NSCameraUsageDescription`).
    NotDetermined,
    Authorized,
    Denied,
    /// Blocked by policy (MDM/Screen Time); the user cannot grant it.
    Restricted,
}

impl CameraPermission {
    /// Maps `AVAuthorizationStatus` raw values (0 notDetermined, 1 restricted, 2 denied,
    /// 3 authorized). Unknown values are treated as not determined.
    pub fn from_av_status(raw: i64) -> Self {
        match raw {
            1 => CameraPermission::Restricted,
            2 => CameraPermission::Denied,
            3 => CameraPermission::Authorized,
            _ => CameraPermission::NotDetermined,
        }
    }

    pub fn is_authorized(self) -> bool {
        self == CameraPermission::Authorized
    }

    /// `Ok` when authorized, otherwise the `permission_missing(camera)` error with a message that
    /// says how to fix it (CAM-4 AC3: "Open System Settings").
    pub fn require(self) -> Result<(), CmError> {
        let msg = match self {
            CameraPermission::Authorized => return Ok(()),
            CameraPermission::NotDetermined => {
                "Camera access has not been requested yet. Allow camera access when prompted."
            }
            CameraPermission::Denied => {
                "Camera access is denied. Enable it in System Settings > Privacy & Security > Camera."
            }
            CameraPermission::Restricted => {
                "Camera access is restricted by a device policy and cannot be granted here."
            }
        };
        Err(CmError::permission_missing(Permission::Camera, msg))
    }
}

/// Current camera permission without prompting.
pub fn check_camera_permission() -> CameraPermission {
    crate::platform::permission_status()
}

/// Asks the user for camera access (shows the system prompt only when not determined) and returns
/// the resulting state. Blocks until the user answers or `timeout` passes. Call from a bundle that
/// carries `NSCameraUsageDescription`.
pub fn request_camera_access(timeout: std::time::Duration) -> CameraPermission {
    crate::platform::request_access(timeout)
}

#[cfg(test)]
mod tests {
    use cm_core::ErrorCode;

    use super::*;

    #[test]
    fn av_status_mapping() {
        assert_eq!(
            CameraPermission::from_av_status(0),
            CameraPermission::NotDetermined
        );
        assert_eq!(
            CameraPermission::from_av_status(1),
            CameraPermission::Restricted
        );
        assert_eq!(
            CameraPermission::from_av_status(2),
            CameraPermission::Denied
        );
        assert_eq!(
            CameraPermission::from_av_status(3),
            CameraPermission::Authorized
        );
        assert_eq!(
            CameraPermission::from_av_status(99),
            CameraPermission::NotDetermined
        );
    }

    #[test]
    fn only_authorized_passes_and_others_map_to_permission_missing_camera() {
        assert!(CameraPermission::Authorized.require().is_ok());
        for p in [
            CameraPermission::NotDetermined,
            CameraPermission::Denied,
            CameraPermission::Restricted,
        ] {
            let e = p.require().unwrap_err();
            assert_eq!(e.code(), ErrorCode::PermissionMissing);
            let payload = e.to_payload();
            assert_eq!(payload.data.unwrap()["permission"], "camera");
            assert_eq!(e.exit_code(), 4);
        }
    }
}
