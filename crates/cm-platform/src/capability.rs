//! What this build can do on this system, decided in one place.

/// The variable that turns Linux recording off in the app (`CM_LINUX_RECORDING=0`). Release 0.3.0 made Linux recording
/// the default (it was `=1` to try it in the 0.2.x early build); a computer where it misbehaves can still opt out.
pub const LINUX_RECORDING_ENV: &str = "CM_LINUX_RECORDING";

/// Whether screen and audio recording are offered by the app on this system. The one switch the
/// Record sheet, the launcher, the hotkeys and the tray ask. macOS, Windows and Linux record (Linux: PipeWire with
/// GNOME's screen-cast service or the desktop portal, VA-API H.264 or software VP9; `CM_LINUX_RECORDING=0` hides
/// it). Elsewhere it says "not available yet" instead of offering a Start button that fails. (The headless daemon,
/// CLI and MCP are not gated.)
pub fn recording_supported() -> bool {
    supported_for(
        cfg!(target_os = "macos") || cfg!(windows),
        cfg!(target_os = "linux"),
        std::env::var(LINUX_RECORDING_ENV).ok().as_deref(),
    )
}

fn supported_for(records: bool, linux: bool, linux_switch: Option<&str>) -> bool {
    records || (linux && linux_switch.is_none_or(|v| v.trim() != "0"))
}

#[cfg(test)]
mod tests {
    use super::supported_for;

    #[test]
    fn macos_windows_and_linux_record_and_linux_can_opt_out() {
        assert!(supported_for(true, false, None));
        assert!(supported_for(false, true, None));
        assert!(supported_for(false, true, Some("1")));
        assert!(!supported_for(false, true, Some("0")));
        assert!(!supported_for(false, true, Some(" 0 ")));
        assert!(!supported_for(false, false, Some("1")));
    }
}
