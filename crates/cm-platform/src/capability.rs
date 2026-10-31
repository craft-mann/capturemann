//! What this build can do on this system, decided in one place.

/// Whether screen and audio recording work on this system. The one switch the app, the CLI and
/// the daemon ask (the Record sheet, the launcher, hotkeys, the tray). macOS records; Windows and
/// Linux are still being built, so they say "not available yet" instead of offering a Start
/// button that fails. The recorder work flips its system to `true` here when it lands.
pub fn recording_supported() -> bool {
    cfg!(target_os = "macos")
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_macos_records_for_now() {
        assert_eq!(super::recording_supported(), cfg!(target_os = "macos"));
    }
}
