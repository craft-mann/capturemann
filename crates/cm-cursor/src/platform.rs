//! Platform dispatch. macOS: `CGEventTap` + `NSCursor`. Windows: low-level hook + `GetCursorInfo`.

#[cfg(target_os = "macos")]
#[path = "macos/mod.rs"]
mod imp;

#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

#[cfg(not(any(target_os = "macos", windows)))]
mod imp {
    use cm_core::CmError;

    use crate::recorder::CursorConfig;
    use crate::schema::{CursorStatus, MoveSource};
    use crate::source::{CursorSource, RawSender};

    pub const CLOCK_NAME: &str = "unsupported";
    pub const PLATFORM: &str = "unsupported";

    struct Unsupported;
    impl CursorSource for Unsupported {
        fn start(&mut self, _tx: RawSender) -> Result<CursorStatus, CmError> {
            Err(CmError::not_implemented(
                "cursor telemetry supports macOS and Windows only",
            ))
        }
        fn stop(&mut self) {}
    }

    pub fn default_source(_config: &CursorConfig) -> Box<dyn CursorSource> {
        Box::new(Unsupported)
    }

    pub fn preflight() -> CursorStatus {
        CursorStatus::full(MoveSource::Fake)
    }
}

#[cfg(target_os = "macos")]
pub use imp::main_display_target;
pub use imp::preflight;
pub(crate) use imp::{default_source, CLOCK_NAME, PLATFORM};
