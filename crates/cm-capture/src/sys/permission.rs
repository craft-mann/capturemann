//! Whether this process may capture the screen, answered without prompting.

use cm_core::CmError;

/// `Ok` when screen capture can work; otherwise the error a client shows: `permission_missing`
/// where the OS has a grant (macOS), `internal` when there is no display at all.
pub fn check() -> Result<(), CmError> {
    imp::check()
}

#[cfg(target_os = "macos")]
mod imp {
    use cm_core::{CmError, Permission};

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
    }

    pub fn check() -> Result<(), CmError> {
        // SAFETY: argument-less CoreGraphics query with no preconditions.
        if unsafe { CGPreflightScreenCaptureAccess() } {
            Ok(())
        } else {
            Err(CmError::permission_missing(
                Permission::Screen,
                "Screen Recording permission is not granted. Enable it in System Settings > \
                 Privacy & Security > Screen Recording, then restart CaptureMann.",
            ))
        }
    }
}

#[cfg(windows)]
mod imp {
    use cm_core::CmError;

    /// Windows has no screen-capture permission prompt.
    pub fn check() -> Result<(), CmError> {
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::super::session::{self, DisplayServer};
    use cm_core::CmError;

    pub fn check() -> Result<(), CmError> {
        match session::from_env() {
            DisplayServer::X11 => Ok(()),
            // The desktop portal asks the user on the first capture.
            DisplayServer::Wayland => Ok(()),
            // Not an internal failure: this process simply has no desktop to capture (a console, an
            // ssh login, a service). The same code the recording preflight uses.
            DisplayServer::None => Err(CmError::platform_rejected(
                "There is no desktop session to capture here (neither DISPLAY nor WAYLAND_DISPLAY \
                 is set). Run CaptureMann from a logged-in desktop session.",
                Some("no_display_session".into()),
            )),
        }
    }
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
mod imp {
    use cm_core::CmError;

    pub fn check() -> Result<(), CmError> {
        Err(CmError::not_implemented("screen capture on this system"))
    }
}
