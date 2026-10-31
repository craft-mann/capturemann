//! What this build can do on this system, decided in one place.

/// The variable that turns Linux recording on in the app while it is experimental.
pub const LINUX_RECORDING_ENV: &str = "CM_LINUX_RECORDING";

/// Whether screen and audio recording are offered by the app on this system. The one switch the
/// Record sheet, the launcher, the hotkeys and the tray ask. macOS records. Linux has a recorder
/// (software VP9, PipeWire; tested in a VM only) that the early build keeps hidden: set
/// `CM_LINUX_RECORDING=1` to try it. Windows has none yet. Elsewhere it says "not available yet"
/// instead of offering a Start button that fails. (The headless daemon, CLI and MCP are not gated.)
pub fn recording_supported() -> bool {
    supported_for(
        cfg!(target_os = "macos"),
        cfg!(target_os = "linux"),
        std::env::var(LINUX_RECORDING_ENV).ok().as_deref(),
    )
}

fn supported_for(macos: bool, linux: bool, linux_opt_in: Option<&str>) -> bool {
    macos || (linux && linux_opt_in.is_some_and(|v| v.trim() == "1"))
}

#[cfg(test)]
mod tests {
    use super::supported_for;

    #[test]
    fn macos_records_and_linux_only_when_asked() {
        assert!(supported_for(true, false, None));
        assert!(!supported_for(false, true, None));
        assert!(!supported_for(false, true, Some("0")));
        assert!(supported_for(false, true, Some("1")));
        assert!(!supported_for(false, false, Some("1")));
    }
}
