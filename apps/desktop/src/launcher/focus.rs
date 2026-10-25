//! Returning focus to the app that was active before the launcher opened (LNC-1 "Window
//! behavior"). macOS only: the launcher must activate CaptureMann to receive keys (a
//! non-activating panel gets no keyboard input from the window server), so on close the
//! previous app is brought back. Elsewhere the OS hands focus back on its own.

/// Process id of the frontmost app, unless that is this process.
#[cfg(target_os = "macos")]
pub fn frontmost_other_app() -> Option<i32> {
    use objc2_app_kit::NSWorkspace;
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = app.processIdentifier();
    (pid != std::process::id() as i32).then_some(pid)
}

/// Brings the app with this process id to the front.
#[cfg(target_os = "macos")]
#[allow(deprecated)]
pub fn activate(pid: i32) {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
    if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
        app.activateWithOptions(NSApplicationActivationOptions::empty());
    }
}

#[cfg(not(target_os = "macos"))]
pub fn frontmost_other_app() -> Option<i32> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn activate(_pid: i32) {}
