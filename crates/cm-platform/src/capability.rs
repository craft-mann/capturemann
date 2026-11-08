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

/// Whether this session records a separate cursor track (positions, clicks, scroll): every system except a
/// Linux session that is not X11 (a Wayland desktop does not let programs read the pointer; the pointer stays
/// in the video). Same rule as the Linux preflight of the recorder.
pub fn records_cursor_track() -> bool {
    #[cfg(target_os = "linux")]
    {
        let get = |k: &str| std::env::var(k).ok();
        linux_cursor_track(
            get("XDG_SESSION_TYPE").as_deref(),
            get("WAYLAND_DISPLAY").as_deref(),
            get("DISPLAY").as_deref(),
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// The Linux rule of [`records_cursor_track`], from the session variables.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub fn linux_cursor_track(
    xdg_session_type: Option<&str>,
    wayland_display: Option<&str>,
    display: Option<&str>,
) -> bool {
    let set = |v: Option<&str>| v.is_some_and(|s| !s.is_empty());
    match xdg_session_type {
        Some("x11") => true,
        Some("wayland") => false,
        _ => !set(wayland_display) && set(display),
    }
}

#[cfg(test)]
mod tests {
    use super::{linux_cursor_track, supported_for};

    #[test]
    fn macos_windows_and_linux_record_and_linux_can_opt_out() {
        assert!(supported_for(true, false, None));
        assert!(supported_for(false, true, None));
        assert!(supported_for(false, true, Some("1")));
        assert!(!supported_for(false, true, Some("0")));
        assert!(!supported_for(false, true, Some(" 0 ")));
        assert!(!supported_for(false, false, Some("1")));
    }

    #[test]
    fn only_an_x11_session_records_a_cursor_track() {
        assert!(linux_cursor_track(Some("x11"), None, Some(":0")));
        assert!(!linux_cursor_track(
            Some("wayland"),
            Some("wayland-0"),
            Some(":0")
        ));
        // No XDG_SESSION_TYPE: Wayland wins when both displays are set, as in the recorder.
        assert!(!linux_cursor_track(None, Some("wayland-0"), Some(":0")));
        assert!(linux_cursor_track(None, None, Some(":0")));
        assert!(!linux_cursor_track(None, None, None));
    }
}
