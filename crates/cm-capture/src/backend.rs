use crate::{Capabilities, DisplayInfo, Frame, WindowInfo};
use cm_core::{CmError, PhysicalSize};

/// The OS layer. Implementations must be cheap to call repeatedly.
///
/// Contract: `capture_display` returns an image at the display's physical size (what
/// [`DisplayInfo::bounds`] says); `capture_window` returns the window content at physical size,
/// 1:1 with `WindowInfo::bounds` and without shadow.
pub trait CaptureBackend: Send + Sync {
    /// `Err(permission_missing(screen))` when the OS denies screen capture. Must not panic or
    /// prompt.
    fn check_permission(&self) -> Result<(), CmError>;
    fn displays(&self) -> Result<Vec<DisplayInfo>, CmError>;
    fn windows(&self) -> Result<Vec<WindowInfo>, CmError>;
    fn capture_display(&self, display_id: u32) -> Result<Frame, CmError>;
    fn capture_window(&self, window_id: u32) -> Result<Frame, CmError>;

    /// What this desktop can do (see [`Capabilities`]). The default is the full set of an
    /// id-based desktop without a system picker.
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    /// Shows the desktop's own picker and returns the pixels the person chose (a window, a
    /// region or a screen). Blocks until they choose or give up: `user_declined` when they
    /// cancel, `confirmation_timeout` when nobody answers. `not_implemented` without a system
    /// picker (see [`Capabilities::pick`]).
    fn pick_interactive(&self) -> Result<Frame, CmError> {
        Err(CmError::not_implemented(
            "this desktop has no system picker for screenshots",
        ))
    }

    /// A display frame (same contract as `capture_display`) with the windows of process
    /// `exclude_pid` left out (R4-6: CaptureMann's own toast, dialogs and Studio must not be in
    /// its screenshots). `Ok(None)` means the backend cannot filter by process here (the
    /// default); the caller then uses `capture_display`.
    fn capture_display_excluding(
        &self,
        _display_id: u32,
        _size: PhysicalSize,
        _exclude_pid: u32,
    ) -> Result<Option<Frame>, CmError> {
        Ok(None)
    }

    /// Like [`CaptureBackend::capture_display_excluding`], but the windows in `keep_window_ids`
    /// stay in the frame although their process is excluded (the library window may appear in
    /// a screenshot; the toast and dialogs may not). The default ignores the keep set and
    /// delegates, so existing backends keep working.
    fn capture_display_excluding_except(
        &self,
        display_id: u32,
        size: PhysicalSize,
        exclude_pid: u32,
        _keep_window_ids: &[u32],
    ) -> Result<Option<Frame>, CmError> {
        self.capture_display_excluding(display_id, size, exclude_pid)
    }
}
