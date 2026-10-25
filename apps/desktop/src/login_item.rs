//! "Launch at login" (CAP-2.AC5): a LaunchAgent that starts the bundled app in the tray
//! (`--background`, no window) when the user logs in. Default OFF until the user turns it on in Settings: a
//! login item is persistence on the user's machine, so it needs an explicit opt-in (CAP-2.AC5 says "default on"
//! for an installed release; revisit when there is an installer).
//!
//! Only a copy running from a `CaptureMann.app` registers (a dev binary in `target/` must never
//! install a login item that points at it). The choice is kept in a file beside the settings
//! (`launch_at_login`: `on` or `off`; absent means off), not in `settings.json`, so it is
//! not a setting an agent can change through `set_settings`: starting at login is persistence.

use std::path::{Path, PathBuf};

/// The LaunchAgent label: the bundle id.
pub const LABEL: &str = "dev.capturemann.app";
/// The preference file in the config directory.
pub const PREF_FILE: &str = "launch_at_login";

/// What to do about the LaunchAgent file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Write this plist (missing, or pointing somewhere else).
    Install(String),
    /// Delete the plist.
    Remove,
    Nothing,
}

/// The user's choice: off unless the preference file says `on`.
pub fn enabled(pref_file_text: Option<&str>) -> bool {
    pref_file_text.map(str::trim) == Some("on")
}

/// The LaunchAgent plist for the app at `exe` (`X.app/Contents/MacOS/cm-desktop`).
pub fn plist(exe: &Path) -> String {
    let exe = exe
        .to_string_lossy()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
\t<key>Label</key>\n\t<string>{LABEL}</string>\n\
\t<key>ProgramArguments</key>\n\t<array>\n\t\t<string>{exe}</string>\n\t\t<string>--background</string>\n\t</array>\n\
\t<key>RunAtLoad</key>\n\t<true/>\n\
\t<key>ProcessType</key>\n\t<string>Interactive</string>\n\
</dict>\n\
</plist>\n"
    )
}

/// `~/Library/LaunchAgents/<label>.plist`.
pub fn plist_path(home: &Path) -> PathBuf {
    home.join("Library")
        .join("LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

/// What to do, given the choice, the app's executable when it runs from a bundle (`None`: a bare
/// binary, which never registers and never removes someone else's entry) and the current file.
pub fn plan(enabled: bool, bundled_exe: Option<&Path>, current: Option<&str>) -> Plan {
    let Some(exe) = bundled_exe else {
        return Plan::Nothing;
    };
    if !enabled {
        return if current.is_some() {
            Plan::Remove
        } else {
            Plan::Nothing
        };
    }
    let want = plist(exe);
    if current == Some(want.as_str()) {
        Plan::Nothing
    } else {
        Plan::Install(want)
    }
}

/// The executable when it lives in a `.app` bundle.
#[cfg(any(target_os = "macos", test))]
fn bundled_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    crate::ui::app::bundle_of(&exe).map(|_| exe)
}

/// Reads the preference from `config_dir`.
pub fn read_enabled(config_dir: &Path) -> bool {
    enabled(
        std::fs::read_to_string(config_dir.join(PREF_FILE))
            .ok()
            .as_deref(),
    )
}

/// Brings the LaunchAgent in line with the preference (at startup, and after the toggle). A
/// failure is returned as text for the log or the toggle's note; it never stops the app.
pub fn sync(config_dir: &Path) -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = config_dir;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let Some(home) = dirs::home_dir() else {
            return Ok(());
        };
        let path = plist_path(&home);
        let current = std::fs::read_to_string(&path).ok();
        match plan(
            read_enabled(config_dir),
            bundled_exe().as_deref(),
            current.as_deref(),
        ) {
            Plan::Nothing => Ok(()),
            Plan::Remove => std::fs::remove_file(&path).map_err(|e| e.to_string()),
            Plan::Install(text) => {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                std::fs::write(&path, text).map_err(|e| e.to_string())
            }
        }
    }
}

/// Saves the user's choice and applies it.
pub fn set_enabled(config_dir: &Path, on: bool) -> Result<(), String> {
    std::fs::create_dir_all(config_dir).map_err(|e| e.to_string())?;
    std::fs::write(config_dir.join(PREF_FILE), if on { "on" } else { "off" })
        .map_err(|e| e.to_string())?;
    sync(config_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe() -> PathBuf {
        PathBuf::from("/Applications/CaptureMann.app/Contents/MacOS/cm-desktop")
    }

    /// CAP-2.AC5: default off (explicit opt-in); the plist starts the app in the tray with no window.
    #[test]
    fn cap2_ac5_default_off_and_background_start() {
        assert!(!enabled(None));
        assert!(enabled(Some("on\n")));
        assert!(!enabled(Some("off\n")));
        let p = plist(&exe());
        assert!(p.contains("<key>RunAtLoad</key>\n\t<true/>"));
        assert!(p.contains("<string>--background</string>"));
        assert!(p.contains(&format!("<string>{LABEL}</string>")));
        assert!(p.contains("/Applications/CaptureMann.app/Contents/MacOS/cm-desktop"));
        assert!(plist_path(Path::new("/Users/a"))
            .ends_with("Library/LaunchAgents/dev.capturemann.app.plist"));
    }

    #[test]
    fn plan_installs_updates_and_removes() {
        let e = exe();
        let want = plist(&e);
        assert_eq!(plan(true, Some(&e), None), Plan::Install(want.clone()));
        assert_eq!(plan(true, Some(&e), Some(&want)), Plan::Nothing);
        assert_eq!(
            plan(true, Some(&e), Some("old")),
            Plan::Install(want),
            "an app moved to another folder rewrites it"
        );
        assert_eq!(plan(false, Some(&e), Some("x")), Plan::Remove);
        assert_eq!(plan(false, Some(&e), None), Plan::Nothing);
    }

    /// A dev binary (no bundle) never installs or removes a login item.
    #[test]
    fn a_bare_binary_never_touches_the_login_item() {
        assert_eq!(plan(true, None, None), Plan::Nothing);
        assert_eq!(plan(false, None, Some("x")), Plan::Nothing);
    }

    #[test]
    fn the_exe_path_is_escaped() {
        let p = plist(Path::new("/A&B/<x>.app/Contents/MacOS/cm-desktop"));
        assert!(p.contains("/A&amp;B/&lt;x&gt;.app"));
    }

    #[test]
    fn the_choice_is_stored_beside_the_settings() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!read_enabled(dir.path()), "off until the user opts in");
        // Not from a bundle in tests: nothing is written to ~/Library.
        set_enabled(dir.path(), true).unwrap();
        assert!(read_enabled(dir.path()));
        set_enabled(dir.path(), false).unwrap();
        assert!(!read_enabled(dir.path()));
    }
}
