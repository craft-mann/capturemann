//! Panic guard for callbacks that foreign code (macOS C, Objective-C, whisper.cpp) calls into.
//!
//! A panic unwinding out of an `extern "C"` function, an Objective-C method or a block aborts the
//! process or is undefined behaviour, and would lose the user's recording. Wrap the body of every
//! such callback in [`guard`] so a panic is caught, logged at error level (stderr in the binaries) and replaced by a safe value.

use std::panic::{catch_unwind, AssertUnwindSafe};

/// Runs `f`; if it panics, logs one line naming `site` and returns `fallback` instead.
///
/// The default panic hook has already printed the message and location by then.
pub fn guard<T>(site: &str, fallback: T, f: impl FnOnce() -> T) -> T {
    // AssertUnwindSafe: on a panic the closure's captures are dropped unused; shared state behind
    // locks is only read through poison-aware `lock()` calls by the callers.
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) => {
            log::error!(
                "capturemann: panic in native callback `{site}` caught; returning a safe default"
            );
            fallback
        }
    }
}

/// Like [`guard`] for callbacks that return nothing.
pub fn guard_unit(site: &str, f: impl FnOnce()) {
    guard(site, (), f);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_the_value_when_nothing_panics() {
        assert_eq!(guard("t", 0, || 7), 7);
    }

    #[test]
    fn a_panic_returns_the_fallback_and_does_not_propagate() {
        let v = guard("t", 42, || -> i32 { panic!("boom") });
        assert_eq!(v, 42);
    }

    #[test]
    fn a_panicking_unit_callback_is_swallowed() {
        let mut ran = false;
        guard_unit("t", || {
            ran = true;
            panic!("boom");
        });
        assert!(ran);
    }
}
