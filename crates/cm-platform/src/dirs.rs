//! The user's standard folders. Reading the environment only; nothing is created.

use std::ffi::OsString;
use std::path::PathBuf;

fn non_empty(var: &str) -> Option<OsString> {
    std::env::var_os(var).filter(|v| !v.is_empty())
}

/// The user's home folder (`USERPROFILE` on Windows, `HOME` elsewhere).
pub fn home_dir() -> Option<PathBuf> {
    non_empty(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// The folder an app keeps its settings and data in: `~/Library/Application Support` on macOS,
/// `%APPDATA%` on Windows, `$XDG_CONFIG_HOME` or `~/.config` elsewhere. The caller adds its
/// own folder name.
pub fn config_base_dir() -> Option<PathBuf> {
    sys::config_base_dir()
}

/// Where recordings go by default: `~/Movies` on macOS, `~/Videos` elsewhere.
pub fn video_dir() -> Option<PathBuf> {
    home_dir().map(|h| {
        h.join(if cfg!(target_os = "macos") {
            "Movies"
        } else {
            "Videos"
        })
    })
}

#[cfg(target_os = "macos")]
mod sys {
    use super::*;

    pub fn config_base_dir() -> Option<PathBuf> {
        home_dir().map(|h| h.join("Library/Application Support"))
    }
}

#[cfg(windows)]
mod sys {
    use super::*;

    pub fn config_base_dir() -> Option<PathBuf> {
        non_empty("APPDATA").map(PathBuf::from)
    }
}

#[cfg(all(not(target_os = "macos"), not(windows)))]
mod sys {
    use super::*;

    pub fn config_base_dir() -> Option<PathBuf> {
        non_empty("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|h| h.join(".config")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folders_are_absolute_when_known() {
        for p in [home_dir(), config_base_dir(), video_dir()]
            .into_iter()
            .flatten()
        {
            assert!(p.is_absolute(), "{}", p.display());
        }
    }
}
