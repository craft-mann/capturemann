//! Keeps an Objective-C exception inside one track from taking the app down.
//!
//! macOS frameworks (AVFoundation, ScreenCaptureKit, Core Audio's wrappers) report some bad
//! states by throwing an `NSException`. One that unwinds into Rust aborts the process, leaving
//! the "Currently Sharing" indicator up (GitHub issue 16: the abort came from a track's start
//! thread after the encoder set-up had been fixed). Every call into a track runs through
//! [`call`], so a throw becomes an ordinary error for that track: an optional track is dropped
//! with a warning and the recording goes on, a required one fails the start cleanly.

use cm_core::CmError;

/// Runs `f` (a call into the track of `what`, for example `"camera start"`); an Objective-C
/// exception it throws becomes an `internal` error with the exception's text.
pub fn call<T>(what: &str, f: impl FnOnce() -> Result<T, CmError>) -> Result<T, CmError> {
    #[cfg(target_os = "macos")]
    {
        match objc2::exception::catch(std::panic::AssertUnwindSafe(f)) {
            Ok(r) => r,
            Err(e) => {
                let text = e.map_or_else(
                    || "an Objective-C exception without details".to_owned(),
                    |e| format!("{e:?}"),
                );
                log::error!("{what}: caught an Objective-C exception: {text}");
                Err(CmError::internal(format!(
                    "{what} failed: macOS threw an exception ({text})"
                )))
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = what;
        f()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_result_passes_through() {
        assert_eq!(call("x", || Ok(3)).unwrap(), 3);
        let e = call::<()>("x", || Err(CmError::invalid_input("no"))).unwrap_err();
        assert!(e.to_string().contains("no"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_thrown_exception_becomes_an_error() {
        use objc2_foundation::{NSException, NSString};
        let e = call::<()>("camera start", || {
            let name = NSString::from_str("TestException");
            let reason = NSString::from_str("bad state");
            // SAFETY: raising on a freshly made exception inside `catch`.
            unsafe {
                NSException::exceptionWithName_reason_userInfo(&name, Some(&reason), None).raise()
            }
        })
        .unwrap_err();
        let s = e.to_string();
        assert!(s.contains("camera start failed"), "{s}");
    }
}
