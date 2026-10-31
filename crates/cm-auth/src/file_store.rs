//! Secrets in a file the owner alone can read: the opt-in for a computer with no system keyring
//! (a Linux window manager without GNOME Keyring, KWallet or KeePassXC, a server, a container).
//! The values are NOT encrypted; the protection is the file mode and the folder. It is never used
//! unless the user chose it, so a missing keyring stays a clear error, not a silent downgrade.

use std::collections::BTreeMap;
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cm_core::CmError;

use crate::SecretStore;

/// The environment variable that chooses the backend (`file`; anything else, or nothing, means the
/// system keyring).
pub const BACKEND_ENV: &str = "CAPTUREMANN_SECRETS";
/// The same choice kept in the config folder (one word, `file`), for people who start the app from a
/// launcher and cannot set variables. Not a setting: no agent can change it through `set_settings`.
pub const BACKEND_FILE: &str = "secrets_backend";
/// The secrets file in the config folder.
pub const SECRETS_FILE: &str = "secrets.json";

/// Which store holds the secrets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Backend {
    /// The system keyring (Keychain, Credential Manager, Secret Service).
    Keyring,
    /// An unencrypted file with owner-only access, at this path.
    File(PathBuf),
}

/// The user's choice: the variable wins, then the file in `config_dir`.
pub fn choose_backend(env: Option<&str>, pref_file: Option<&str>, config_dir: &Path) -> Backend {
    let wants_file = |v: &str| v.trim().eq_ignore_ascii_case("file");
    if env.or(pref_file).is_some_and(wants_file) {
        Backend::File(config_dir.join(SECRETS_FILE))
    } else {
        Backend::Keyring
    }
}

/// [`choose_backend`] on this process's environment and config folder.
pub fn backend_from_env() -> Backend {
    let dir = cm_platform::dirs::app_config_dir();
    let env = std::env::var(BACKEND_ENV).ok();
    let pref = std::fs::read_to_string(dir.join(BACKEND_FILE)).ok();
    choose_backend(env.as_deref(), pref.as_deref(), &dir)
}

/// A JSON object of key to value in one file, replaced atomically on every change.
pub struct FileStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl FileStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: Mutex::new(()),
        }
    }

    fn read(&self) -> Result<BTreeMap<String, String>, CmError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| {
                // Never replaced silently: that would drop every account's secrets.
                CmError::internal(format!(
                    "the secrets file {} cannot be read ({e}); fix or remove it",
                    self.path.display()
                ))
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(CmError::internal(format!(
                "cannot read the secrets file {}: {e}",
                self.path.display()
            ))),
        }
    }

    fn write(&self, map: &BTreeMap<String, String>) -> Result<(), CmError> {
        let fail = |what: &str, e: std::io::Error| {
            CmError::internal(format!(
                "cannot {what} the secrets file {}: {e}",
                self.path.display()
            ))
        };
        let dir = self.path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(dir).map_err(|e| fail("create the folder of", e))?;
        let text = serde_json::to_vec(map)
            .map_err(|e| CmError::internal(format!("cannot encode the secrets: {e}")))?;
        let tmp = self.path.with_extension("json.tmp");
        {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            let mut f = options.open(&tmp).map_err(|e| fail("write", e))?;
            f.write_all(&text).map_err(|e| fail("write", e))?;
            f.sync_all().map_err(|e| fail("write", e))?;
        }
        std::fs::rename(&tmp, &self.path).map_err(|e| fail("replace", e))
    }

    fn locked(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl fmt::Debug for FileStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileStore")
            .field("path", &self.path)
            .finish()
    }
}

impl SecretStore for FileStore {
    fn get(&self, key: &str) -> Result<Option<String>, CmError> {
        let _g = self.locked();
        Ok(self.read()?.get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), CmError> {
        let _g = self.locked();
        let mut map = self.read()?;
        map.insert(key.to_owned(), value.to_owned());
        self.write(&map)
    }

    fn delete(&self, key: &str) -> Result<(), CmError> {
        let _g = self.locked();
        let mut map = self.read()?;
        if map.remove(key).is_some() {
            self.write(&map)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_variable_or_the_file_opts_in_and_nothing_else_does() {
        let dir = Path::new("/c");
        let file = Backend::File(dir.join("secrets.json"));
        assert_eq!(choose_backend(None, None, dir), Backend::Keyring);
        assert_eq!(choose_backend(Some("file"), None, dir), file);
        assert_eq!(choose_backend(Some(" FILE\n"), None, dir), file);
        assert_eq!(choose_backend(None, Some("file\n"), dir), file);
        assert_eq!(
            choose_backend(Some("keyring"), Some("file"), dir),
            Backend::Keyring
        );
        assert_eq!(choose_backend(Some("yes"), None, dir), Backend::Keyring);
        assert_eq!(choose_backend(None, Some(""), dir), Backend::Keyring);
    }

    #[test]
    fn values_round_trip_replace_and_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let s = FileStore::new(tmp.path().join("sub").join("secrets.json"));
        assert_eq!(s.get("a").unwrap(), None);
        s.delete("a").unwrap();
        s.set("a", "one").unwrap();
        s.set("b", "two").unwrap();
        s.set("a", "three").unwrap();
        assert_eq!(s.get("a").unwrap().as_deref(), Some("three"));
        assert_eq!(s.get("b").unwrap().as_deref(), Some("two"));
        // A second store over the same file sees the same data.
        let again = FileStore::new(tmp.path().join("sub").join("secrets.json"));
        assert_eq!(again.get("b").unwrap().as_deref(), Some("two"));
        s.delete("a").unwrap();
        assert_eq!(s.get("a").unwrap(), None);
        assert!(!format!("{s:?}").contains("two"));
        assert!(!tmp.path().join("sub").join("secrets.json.tmp").exists());
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_readable_by_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("secrets.json");
        let s = FileStore::new(&path);
        s.set("k", "v").unwrap();
        s.set("k", "w").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_damaged_file_is_an_error_and_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("secrets.json");
        std::fs::write(&path, "{ not json").unwrap();
        let s = FileStore::new(&path);
        assert!(s.get("k").is_err());
        assert!(s.set("k", "v").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
    }
}
