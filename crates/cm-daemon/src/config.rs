//! Where the app keeps its settings and library, shared by the headless `cm-daemon` and the
//! CLI's on-demand daemon so they behave like the desktop app (R2-8).
//!
//! The rules mirror `apps/desktop/src/settings.rs` (LIB-2):
//!
//! * config folder: `CAPTUREMANN_CONFIG_DIR`, else `<platform config dir>/CaptureMann`
//!   (`~/Library/Application Support` on macOS, `%APPDATA%` on Windows,
//!   `$XDG_CONFIG_HOME` or `~/.config` elsewhere); settings file: `settings.json` in it;
//! * library folder: `CAPTUREMANN_LIBRARY`, else the setting `library.path`, else
//!   `<video folder>/CaptureMann` (`~/Movies` on macOS, `%USERPROFILE%\Videos` on Windows).
//!
//! [`FileSettings`] is a small `settings.json` store with the same file format as the app's
//! (a flat JSON object of dotted keys), so a headless daemon keeps settings (and the 60 fps
//! self-test verdict) across runs and sees what the app saved. It is meant for one process at
//! a time: the app and a headless daemon do not serve the same socket together. The desktop app
//! keeps its own richer store; both read and write the same file shape.

use cm_core::CmError;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use crate::engines::SettingsStore;

/// App folder name inside the platform config and data directories.
pub const APP_DIR: &str = "CaptureMann";
/// The setting that moves the library (app-only for agents, see
/// [`crate::engines::APP_ONLY_SETTINGS`]).
pub const KEY_LIBRARY_PATH: &str = "library.path";

fn home() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn platform_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        home().map(|h| h.join("Library/Application Support"))
    }
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join(".config")))
    }
}

fn platform_video_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        home().map(|h| h.join("Movies"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        home().map(|h| h.join("Videos"))
    }
}

/// The app's config folder (see the module docs).
pub fn config_dir() -> PathBuf {
    std::env::var_os("CAPTUREMANN_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| platform_config_dir().map(|d| d.join(APP_DIR)))
        .unwrap_or_else(|| std::env::temp_dir().join(APP_DIR))
}

/// `settings.json` in [`config_dir`].
pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

/// LIB-2 default: `<video folder>/CaptureMann`, the home folder's `Videos` when the video
/// folder is unknown.
pub fn default_library_dir(video_dir: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    video_dir
        .or_else(|| home.map(|h| h.join("Videos")))
        .map(|d| d.join(APP_DIR))
}

/// Where the library lives: the override, then `library.path`, then `default`.
pub fn resolve_library_path(
    env_override: Option<PathBuf>,
    settings: &Map<String, Value>,
    default: Option<PathBuf>,
) -> Option<PathBuf> {
    env_override
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| {
            settings
                .get(KEY_LIBRARY_PATH)
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from)
        })
        .or(default)
}

/// The library folder a daemon started without `--library` uses: the same answer as the
/// desktop app (`CAPTUREMANN_LIBRARY`, `library.path` in [`settings_path`], the LIB-2 default).
pub fn default_library_path() -> Option<PathBuf> {
    let settings = FileSettings::load(settings_path()).snapshot();
    resolve_library_path(
        std::env::var_os("CAPTUREMANN_LIBRARY").map(PathBuf::from),
        &settings,
        default_library_dir(platform_video_dir(), home()),
    )
}

/// A `settings.json` store: reads served from memory, every `set` re-reads the file, merges and
/// writes atomically (temp file, then rename) with owner-only permissions.
#[derive(Debug)]
pub struct FileSettings {
    path: PathBuf,
    map: Mutex<Map<String, Value>>,
}

fn read_map(path: &Path) -> Map<String, Value> {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(m)) => m,
            _ => {
                // Keep the broken file so nothing is lost silently.
                let _ = std::fs::rename(path, path.with_file_name("settings.broken.json"));
                Map::new()
            }
        },
        Err(_) => Map::new(),
    }
}

impl FileSettings {
    /// Loads `path`; a missing file is empty settings, an unparsable one is moved to
    /// `settings.broken.json`.
    pub fn load(path: PathBuf) -> FileSettings {
        let map = read_map(&path);
        FileSettings {
            path,
            map: Mutex::new(map),
        }
    }

    /// The current settings.
    pub fn snapshot(&self) -> Map<String, Value> {
        self.map
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn write(&self, map: &Map<String, Value>) -> Result<(), CmError> {
        let io = |e: std::io::Error| CmError::internal(format!("cannot save settings: {e}"));
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(&Value::Object(map.clone()))
            .map_err(|e| CmError::internal(e.to_string()))?;
        write_private(&tmp, text.as_bytes()).map_err(io)?;
        std::fs::rename(&tmp, &self.path).map_err(io)
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

impl SettingsStore for FileSettings {
    fn all(&self) -> Result<Map<String, Value>, CmError> {
        Ok(self.snapshot())
    }

    fn set(&self, values: &Map<String, Value>) -> Result<Map<String, Value>, CmError> {
        let mut g = self.map.lock().unwrap_or_else(PoisonError::into_inner);
        // Merge into what is on disk now, so a change the app saved meanwhile is not lost.
        let mut next = read_map(&self.path);
        for (k, v) in g.iter() {
            next.entry(k.clone()).or_insert_with(|| v.clone());
        }
        for (k, v) in values {
            next.insert(k.clone(), v.clone());
        }
        self.write(&next)?;
        *g = next.clone();
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn obj(v: Value) -> Map<String, Value> {
        match v {
            Value::Object(m) => m,
            _ => Map::new(),
        }
    }

    /// R2-8: the same LIB-2 rules as the desktop app.
    #[test]
    fn r2_8_library_rules_match_the_app() {
        assert_eq!(
            default_library_dir(Some(p("/Users/a/Movies")), Some(p("/Users/a"))),
            Some(p("/Users/a/Movies/CaptureMann"))
        );
        assert_eq!(
            default_library_dir(None, Some(p("/home/a"))),
            Some(p("/home/a/Videos/CaptureMann"))
        );
        assert_eq!(default_library_dir(None, None), None);
        let m = obj(json!({"library.path": "/Volumes/Big/Caps"}));
        let d = Some(p("/def"));
        assert_eq!(
            resolve_library_path(Some(p("/env")), &m, d.clone()),
            Some(p("/env"))
        );
        assert_eq!(
            resolve_library_path(None, &m, d.clone()),
            Some(p("/Volumes/Big/Caps"))
        );
        assert_eq!(resolve_library_path(None, &Map::new(), d), Some(p("/def")));
        assert_eq!(resolve_library_path(Some(p("")), &Map::new(), None), None);
        assert_eq!(
            resolve_library_path(None, &obj(json!({"library.path": "  "})), Some(p("/d"))),
            Some(p("/d"))
        );
    }

    /// R2-8: settings (the 60 fps verdict included) survive a restart and the file is private.
    #[test]
    fn r2_8_file_settings_persist_merge_and_are_private() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("cfg/settings.json");
        let s = FileSettings::load(path.clone());
        assert!(s.all().unwrap().is_empty());
        s.set(&obj(
            json!({"recording.fps": 30, "recording.fps60_selftest": {"v": 1}}),
        ))
        .unwrap();
        // Another writer (the app) adds a key meanwhile.
        let mut on_disk = read_map(&path);
        on_disk.insert("appearance.theme".into(), json!("dark"));
        std::fs::write(&path, serde_json::to_string(&on_disk).unwrap()).unwrap();
        let after = s.set(&obj(json!({"recording.size": "1080p"}))).unwrap();
        assert_eq!(after["appearance.theme"], "dark");
        assert_eq!(after["recording.fps"], 30);
        let again = FileSettings::load(path.clone());
        assert_eq!(
            again.all().unwrap()["recording.fps60_selftest"],
            json!({"v": 1})
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "{mode:o}");
        }
    }

    #[test]
    fn broken_file_is_kept_aside() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("settings.json");
        std::fs::write(&path, "{nope").unwrap();
        let s = FileSettings::load(path);
        assert!(s.all().unwrap().is_empty());
        assert!(t.path().join("settings.broken.json").exists());
    }
}
