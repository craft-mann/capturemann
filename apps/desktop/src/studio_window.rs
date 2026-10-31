//! The library (Studio) window as the recorder sees it: the one window of this process that a
//! recording or a screenshot still captures (REC-6.AC8). Everything else CaptureMann draws while
//! recording (HUD, countdown, area indicator, webcam bubble, start sheet, toasts, confirmation
//! dialogs, launcher) is excluded by the engine's pid rule and never goes through here.
//!
//! The provider the daemon asks (`DaemonBuilder::kept_windows`) is called from capture threads,
//! but `NSWindow` may only be touched on the main thread. So the main thread publishes the
//! window's CGWindowID into an atomic ([`publish`], from `ui::app`'s watch, twice a second) and
//! the provider only reads that atomic. Fail-safe: no id (window closed, minimized, not yet
//! published) means it is excluded like every other own window.

use std::sync::atomic::{AtomicU32, Ordering};

/// 0 = no visible Studio window.
static STUDIO_WINDOW_ID: AtomicU32 = AtomicU32::new(0);

/// Records the Studio window's CGWindowID (`None`: closed, hidden or minimized). Only
/// `ui::app` calls this, and only with the Studio window.
pub fn publish(id: Option<u32>) {
    STUDIO_WINDOW_ID.store(id.unwrap_or(0), Ordering::SeqCst);
}

/// The kept-window provider handed to the daemon: the Studio window when it is visible, nothing
/// else.
pub fn kept_windows() -> Vec<u32> {
    // Linux: the engine asks before every display capture and target listing, which is the
    // moment to take the pop-ups off the screen (a no-op where the OS can exclude them).
    crate::capture_cloak::before_capture();
    match STUDIO_WINDOW_ID.load(Ordering::SeqCst) {
        0 => Vec::new(),
        id => vec![id],
    }
}

/// The CGWindowID to publish for a window with this `NSWindow.windowNumber` and state:
/// `None` unless it is on screen (visible, not minimized) and has a real number (AppKit reports
/// -1 or 0 for a window without a window-server window).
pub fn capturable_id(number: isize, visible: bool, minimized: bool) -> Option<u32> {
    if !visible || minimized || number <= 0 {
        return None;
    }
    u32::try_from(number).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window id rule: visible, not minimized, a real window number.
    #[test]
    fn only_a_visible_window_has_an_id() {
        assert_eq!(capturable_id(4711, true, false), Some(4711));
        assert_eq!(capturable_id(4711, false, false), None);
        assert_eq!(capturable_id(4711, true, true), None);
        assert_eq!(capturable_id(-1, true, false), None);
        assert_eq!(capturable_id(0, true, false), None);
    }

    /// REC-6.AC8: the provider answers the Studio window and nothing else; a closed window
    /// clears it. (One test: the state is process-wide.)
    #[test]
    fn provider_returns_only_the_studio_window() {
        publish(None);
        assert!(kept_windows().is_empty(), "nothing published: all excluded");
        publish(Some(4711));
        assert_eq!(kept_windows(), vec![4711]);
        publish(None);
        assert!(kept_windows().is_empty());
    }

    /// No recorder surface may publish a window: `studio_window::publish` is called from the
    /// Studio watch in `ui/app/watch.rs` only, and the recorder modules never touch this module.
    #[test]
    fn recorder_surfaces_never_publish_windows() {
        use std::path::Path;
        fn scan(dir: &Path, hits: &mut Vec<String>) {
            let Ok(rd) = std::fs::read_dir(dir) else {
                return;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    scan(&p, hits);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let name = p.to_string_lossy().into_owned();
                    let text = std::fs::read_to_string(&p).unwrap_or_default();
                    if text.contains("studio_window::publish") {
                        hits.push(name);
                    }
                }
            }
        }
        let mut hits = Vec::new();
        scan(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut hits,
        );
        assert!(
            hits.iter().all(|h| {
                let h = Path::new(h);
                h.ends_with("ui/app/watch.rs") || h.ends_with("studio_window.rs")
            }),
            "only ui/app/watch.rs may publish: {hits:?}"
        );
    }
}
