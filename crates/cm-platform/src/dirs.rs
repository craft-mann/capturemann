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

/// The app's config folder: `CAPTUREMANN_CONFIG_DIR`, else `<config base>/CaptureMann` (the rule the
/// settings file follows); the temp folder when the system names no config base.
pub fn app_config_dir() -> PathBuf {
    if let Some(d) = non_empty("CAPTUREMANN_CONFIG_DIR") {
        return PathBuf::from(d);
    }
    config_base_dir()
        .map(|b| b.join("CaptureMann"))
        .unwrap_or_else(|| std::env::temp_dir().join("CaptureMann"))
}

/// Where recordings go by default: `~/Movies` on macOS, the `XDG_VIDEOS_DIR` of
/// `$XDG_CONFIG_HOME/user-dirs.dirs` on Linux (a localised desktop may name it `~/Vidéos`),
/// `~/Videos` otherwise.
pub fn video_dir() -> Option<PathBuf> {
    Some(video_dir_in(&home_dir()?, config_base_dir().as_deref()))
}

fn video_dir_in(home: &std::path::Path, config_base: Option<&std::path::Path>) -> PathBuf {
    if cfg!(target_os = "macos") {
        return home.join("Movies");
    }
    if cfg!(all(unix, not(target_os = "macos"))) {
        let text = config_base.and_then(|b| std::fs::read_to_string(b.join("user-dirs.dirs")).ok());
        if let Some(d) = text.and_then(|t| parse_user_dir(&t, "XDG_VIDEOS_DIR", home)) {
            return d;
        }
    }
    home.join("Videos")
}

/// One folder from the text of an xdg-user-dirs `user-dirs.dirs` file. Lines look like
/// `XDG_VIDEOS_DIR="$HOME/Videos"`: the value is double-quoted, may start with `$HOME`, and
/// `\"` / `\\` are escapes. Comments and other keys are skipped; the last assignment wins. `None`
/// when the key is missing, the value is empty or not absolute after expansion, or it is the home
/// folder itself (what xdg-user-dirs writes for a folder the user switched off).
pub fn parse_user_dir(text: &str, key: &str, home: &std::path::Path) -> Option<PathBuf> {
    let mut found = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k.trim() != key {
            continue;
        }
        found = unquote_user_dir(v.trim());
    }
    let raw = found?;
    let path = if raw == "$HOME" {
        home.to_path_buf()
    } else if let Some(rest) = raw.strip_prefix("$HOME/") {
        home.join(rest)
    } else {
        PathBuf::from(&raw)
    };
    if raw.is_empty() || !path.is_absolute() || path == home {
        return None;
    }
    Some(path)
}

fn unquote_user_dir(v: &str) -> Option<String> {
    let inner = v.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = inner.chars();
    loop {
        match chars.next()? {
            '"' => return Some(out),
            '\\' => out.push(chars.next()?),
            c => out.push(c),
        }
    }
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
#[allow(clippy::unwrap_used)]
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

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_localised_videos_folder_is_read_from_the_user_dirs_file() {
        let cfg = tempfile::tempdir().unwrap();
        let home = std::path::Path::new("/home/a");
        // No file: the default.
        assert_eq!(video_dir_in(home, Some(cfg.path())), home.join("Videos"));
        assert_eq!(video_dir_in(home, None), home.join("Videos"));
        std::fs::write(
            cfg.path().join("user-dirs.dirs"),
            "XDG_DESKTOP_DIR=\"$HOME/Bureau\"\nXDG_VIDEOS_DIR=\"$HOME/Vid\u{e9}os\"\n",
        )
        .unwrap();
        assert_eq!(
            video_dir_in(home, Some(cfg.path())),
            home.join("Vid\u{e9}os")
        );
        // A folder switched off (set to $HOME) falls back to the default.
        std::fs::write(
            cfg.path().join("user-dirs.dirs"),
            "XDG_VIDEOS_DIR=\"$HOME/\"\n",
        )
        .unwrap();
        assert_eq!(video_dir_in(home, Some(cfg.path())), home.join("Videos"));
    }

    #[test]
    fn user_dirs_file_rules() {
        let home = std::path::Path::new("/home/a");
        let p = |t: &str| parse_user_dir(t, "XDG_VIDEOS_DIR", home);
        assert_eq!(
            p("XDG_VIDEOS_DIR=\"$HOME/Vidéos\"\n"),
            Some("/home/a/Vidéos".into())
        );
        assert_eq!(
            p("# c\nXDG_VIDEOS_DIR=\"/data/Vid eos\"\n"),
            Some("/data/Vid eos".into())
        );
        assert_eq!(
            p("XDG_VIDEOS_DIR=\"$HOME/x\\\"y\"\n"),
            Some("/home/a/x\"y".into())
        );
        // The last assignment wins; other keys are ignored.
        assert_eq!(
            p("XDG_VIDEOS_DIR=\"$HOME/a\"\nXDG_MUSIC_DIR=\"$HOME/m\"\nXDG_VIDEOS_DIR=\"$HOME/b\""),
            Some("/home/a/b".into())
        );
        assert_eq!(p(""), None);
        assert_eq!(p("XDG_VIDEOS_DIR=\"\""), None);
        assert_eq!(p("XDG_VIDEOS_DIR=\"$HOME\""), None);
        assert_eq!(p("XDG_VIDEOS_DIR=\"$HOME/\""), None);
        assert_eq!(p("XDG_VIDEOS_DIR=\"relative/dir\""), None);
        assert_eq!(p("XDG_VIDEOS_DIR=$HOME/unquoted"), None);
        assert_eq!(p("XDG_VIDEOS_DIR=\"/unterminated"), None);
    }
}
