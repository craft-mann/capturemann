//! Secret storage by key name.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use cm_core::CmError;

/// Where secrets live. Callers pass a key name (the only thing config files keep).
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, CmError>;
    fn set(&self, key: &str, value: &str) -> Result<(), CmError>;
    /// Deleting a key that does not exist is not an error.
    fn delete(&self, key: &str) -> Result<(), CmError>;
}

/// The OS credential store: macOS Keychain, Windows Credential Manager.
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    /// `service` groups this app's entries, for example `dev.capturemann.app`.
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry, CmError> {
        keyring::Entry::new(&self.service, key).map_err(map_err)
    }
}

impl fmt::Debug for KeyringStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyringStore")
            .field("service", &self.service)
            .finish()
    }
}

fn map_err(e: keyring::Error) -> CmError {
    // The keyring error text names the store and the platform, never the secret.
    CmError::internal(format!("credential store: {e}"))
}

impl SecretStore for KeyringStore {
    fn get(&self, key: &str) -> Result<Option<String>, CmError> {
        match self.entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), CmError> {
        self.entry(key)?.set_password(value).map_err(map_err)
    }

    fn delete(&self, key: &str) -> Result<(), CmError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_err(e)),
        }
    }
}

/// All of an app's secrets in ONE keychain item.
///
/// A keychain asks for the user's password once per item, so a store with one item per secret
/// (two or three per connected account) prompted again and again. `VaultStore` keeps every secret
/// in a single JSON blob under one slot of the inner store (the OS keychain), so one "Always Allow"
/// covers everything and one sign-in causes one prompt, not one per secret.
///
/// * Reads are served from a cache that is shared by every `VaultStore` of the same service in
///   this process (the desktop flows and the daemon's token source see each other's writes); a key
///   that is not in the cache is looked up again once before it is called missing, and then not
///   again for [`MISS_WINDOW`] (so a probe for an account that has no such secret does not read the
///   keychain on every call), and the old one-item-per-secret layout is checked once per key.
///   Every keychain read can ask the user for the password, so the number of reads matters.
/// * Writes read the stored blob fresh, change one key and write it back under a lock, so two
///   writers in one process never lose each other's keys.
/// * Items written by the earlier one-item-per-secret layout are moved into the blob the first time
///   their key is read (the user is asked for the password one last time for each), then deleted.
/// * A blob that cannot be read is an error that says so; it is never silently replaced, because
///   that would drop every account's secrets.
pub struct VaultStore {
    inner: Arc<dyn SecretStore>,
    slot: String,
    state: VaultState,
    miss_window: Duration,
}

/// What a process knows about the vault.
#[derive(Default)]
struct VaultCache {
    map: Option<BTreeMap<String, String>>,
    /// Keys a fresh read did not find, and when.
    misses: HashMap<String, Instant>,
    /// Keys whose old-layout item was already looked for.
    legacy_done: HashSet<String>,
    /// When the blob was last read from or written to the keychain.
    synced_at: Option<Instant>,
}

type VaultState = Arc<Mutex<VaultCache>>;

/// How long a key that a fresh read did not find is answered "missing" without another read.
pub const MISS_WINDOW: Duration = Duration::from_secs(30);

/// The slot name inside the keychain service.
pub const VAULT_SLOT: &str = "vault";

#[derive(serde::Serialize, serde::Deserialize)]
struct VaultBody {
    version: u32,
    secrets: BTreeMap<String, String>,
}

impl VaultStore {
    /// The OS keychain, one item called `vault` in `service` (for example `dev.capturemann.app`).
    /// Instances for the same service share their cache and lock inside this process.
    pub fn system(service: impl Into<String>) -> Self {
        static STATES: OnceLock<Mutex<HashMap<String, VaultState>>> = OnceLock::new();
        let service = service.into();
        let state = {
            let mut all = STATES
                .get_or_init(Default::default)
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            all.entry(service.clone()).or_default().clone()
        };
        VaultStore {
            inner: Arc::new(KeyringStore::new(service)),
            slot: VAULT_SLOT.to_owned(),
            state,
            miss_window: MISS_WINDOW,
        }
    }

    /// A vault over any store, with its own cache (tests).
    pub fn over(inner: Arc<dyn SecretStore>) -> Self {
        VaultStore {
            inner,
            slot: VAULT_SLOT.to_owned(),
            state: Arc::default(),
            miss_window: MISS_WINDOW,
        }
    }

    /// The same vault with another miss window (tests that need a second instance's write seen at
    /// once use `Duration::ZERO`).
    pub fn with_miss_window(mut self, window: Duration) -> Self {
        self.miss_window = window;
        self
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VaultCache> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn check_key(&self, key: &str) -> Result<(), CmError> {
        if key == self.slot {
            return Err(CmError::invalid_input(format!(
                "`{key}` is reserved for the secret vault"
            )));
        }
        Ok(())
    }

    /// Reads the blob from the inner store (no cache).
    fn read(&self) -> Result<BTreeMap<String, String>, CmError> {
        let Some(text) = self.inner.get(&self.slot)? else {
            return Ok(BTreeMap::new());
        };
        serde_json::from_str::<VaultBody>(&text)
            .map(|b| b.secrets)
            .map_err(|_| {
                CmError::internal(
                    "the stored secrets could not be read (the keychain item is damaged); \
                     remove the \"vault\" item of this app in Keychain Access and connect again",
                )
            })
    }

    fn write(&self, map: &BTreeMap<String, String>) -> Result<(), CmError> {
        if map.is_empty() {
            return self.inner.delete(&self.slot);
        }
        let body = serde_json::to_string(&VaultBody {
            version: 1,
            secrets: map.clone(),
        })
        .map_err(|e| CmError::internal(format!("credential store: {e}")))?;
        self.inner.set(&self.slot, &body)
    }
}

impl fmt::Debug for VaultStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys: Vec<String> = self
            .lock()
            .map
            .as_ref()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        f.debug_struct("VaultStore")
            .field("slot", &self.slot)
            .field("cached_keys", &keys)
            .finish()
    }
}

impl SecretStore for VaultStore {
    fn get(&self, key: &str) -> Result<Option<String>, CmError> {
        self.check_key(key)?;
        let mut cache = self.lock();
        if cache.map.is_none() {
            cache.map = Some(self.read()?);
            cache.synced_at = Some(Instant::now());
        }
        if let Some(v) = cache.map.as_ref().and_then(|m| m.get(key)) {
            return Ok(Some(v.clone()));
        }
        // A key that was just looked for and not found stays missing for a while.
        if cache
            .misses
            .get(key)
            .is_some_and(|at| at.elapsed() < self.miss_window)
        {
            return Ok(None);
        }
        // Not cached: another writer (the daemon, the connect flow) may have added it, or it may
        // still be an item of the old layout. A read made a moment ago is fresh enough.
        let recent = cache
            .synced_at
            .is_some_and(|at| at.elapsed() < self.miss_window);
        let mut fresh = if recent {
            cache.map.clone().unwrap_or_default()
        } else {
            self.read()?
        };
        if let Some(v) = fresh.get(key).cloned() {
            cache.map = Some(fresh);
            cache.synced_at = Some(Instant::now());
            return Ok(Some(v));
        }
        let legacy = if cache.legacy_done.insert(key.to_owned()) {
            self.inner.get(key)?
        } else {
            None
        };
        if let Some(v) = &legacy {
            fresh.insert(key.to_owned(), v.clone());
            self.write(&fresh)?;
            // Moved: the old item goes away so it never asks again.
            let _ = self.inner.delete(key);
        } else {
            cache.misses.insert(key.to_owned(), Instant::now());
        }
        if !recent {
            cache.synced_at = Some(Instant::now());
        }
        cache.map = Some(fresh);
        Ok(legacy)
    }

    fn set(&self, key: &str, value: &str) -> Result<(), CmError> {
        self.check_key(key)?;
        let mut cache = self.lock();
        let mut map = self.read()?;
        map.insert(key.to_owned(), value.to_owned());
        self.write(&map)?;
        // An item of the old layout under this key is now stale (looked for once per key).
        if cache.legacy_done.insert(key.to_owned()) {
            let _ = self.inner.delete(key);
        }
        cache.misses.remove(key);
        cache.map = Some(map);
        cache.synced_at = Some(Instant::now());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), CmError> {
        self.check_key(key)?;
        let mut cache = self.lock();
        let mut map = self.read()?;
        if map.remove(key).is_some() {
            self.write(&map)?;
        }
        if cache.legacy_done.insert(key.to_owned()) {
            let _ = self.inner.delete(key);
        }
        cache.map = Some(map);
        cache.synced_at = Some(Instant::now());
        Ok(())
    }
}

/// In-memory store for tests.
#[derive(Default)]
pub struct MemoryStore {
    map: Mutex<HashMap<String, String>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.map.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut keys: Vec<String> = self.lock().keys().cloned().collect();
        keys.sort();
        f.debug_struct("MemoryStore").field("keys", &keys).finish()
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, key: &str) -> Result<Option<String>, CmError> {
        Ok(self.lock().get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), CmError> {
        self.lock().insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), CmError> {
        self.lock().remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trip_and_debug_hides_values() {
        let s = MemoryStore::new();
        assert_eq!(s.get("k").unwrap(), None);
        s.set("k", "super-secret-value").unwrap();
        assert_eq!(s.get("k").unwrap().as_deref(), Some("super-secret-value"));
        let dbg = format!("{s:?}");
        assert!(dbg.contains("\"k\""));
        assert!(!dbg.contains("super-secret-value"));
        s.delete("k").unwrap();
        s.delete("k").unwrap();
        assert_eq!(s.get("k").unwrap(), None);
    }

    /// Touches the real keychain (may prompt): only with `-- --ignored`.
    #[test]
    #[ignore = "touches the real OS keychain"]
    fn keyring_store_round_trip() {
        let s = KeyringStore::new("dev.capturemann.cm-auth-test");
        let key = format!("test.{}", std::process::id());
        if let Err(e) = s.set(&key, "v") {
            println!("SKIPPED: credential store unavailable: {e}");
            return;
        }
        assert_eq!(s.get(&key).unwrap().as_deref(), Some("v"));
        s.delete(&key).unwrap();
        assert_eq!(s.get(&key).unwrap(), None);
    }
}

#[cfg(test)]
mod vault_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A store that counts reads and writes of its items (the keychain asks per item access).
    #[derive(Default)]
    struct Counting {
        inner: MemoryStore,
        gets: AtomicUsize,
    }

    impl SecretStore for Counting {
        fn get(&self, key: &str) -> Result<Option<String>, CmError> {
            self.gets.fetch_add(1, Ordering::SeqCst);
            self.inner.get(key)
        }
        fn set(&self, key: &str, value: &str) -> Result<(), CmError> {
            self.inner.set(key, value)
        }
        fn delete(&self, key: &str) -> Result<(), CmError> {
            self.inner.delete(key)
        }
    }

    fn keys(store: &Counting) -> Vec<String> {
        let mut ks: Vec<String> = ["vault", "oauth.google.a", "oauth.google.a.client", "x"]
            .iter()
            .filter(|k| store.inner.get(k).unwrap().is_some())
            .map(|k| (*k).to_owned())
            .collect();
        ks.sort();
        ks
    }

    #[test]
    fn many_secrets_are_one_keychain_item() {
        let inner = Arc::new(Counting::default());
        let v = VaultStore::over(inner.clone());
        v.set("oauth.google.a", "token-a").unwrap();
        v.set("oauth.google.a.client", "client-a").unwrap();
        v.set("x", "y").unwrap();
        assert_eq!(keys(&inner), ["vault"], "only the vault item exists");
        assert_eq!(v.get("oauth.google.a").unwrap().as_deref(), Some("token-a"));
        assert_eq!(v.get("x").unwrap().as_deref(), Some("y"));
        assert_eq!(v.get("nope").unwrap(), None);
    }

    #[test]
    fn reads_come_from_the_cache() {
        let inner = Arc::new(Counting::default());
        let v = VaultStore::over(inner.clone());
        v.set("k", "v").unwrap();
        let before = inner.gets.load(Ordering::SeqCst);
        for _ in 0..20 {
            assert_eq!(v.get("k").unwrap().as_deref(), Some("v"));
        }
        assert_eq!(
            inner.gets.load(Ordering::SeqCst),
            before,
            "no keychain read for a cached key"
        );
    }

    /// Probing for secrets an account does not have (the three providers are tried in turn) must
    /// not read the keychain every time: each read can ask the user for the password.
    #[test]
    fn probing_missing_keys_reads_the_keychain_a_bounded_number_of_times() {
        let inner = Arc::new(Counting::default());
        let seed = VaultStore::over(inner.clone());
        seed.set("oauth.google.a", "t").unwrap();
        let v = VaultStore::over(inner.clone());
        let before = inner.gets.load(Ordering::SeqCst);
        for _ in 0..50 {
            assert_eq!(v.get("oauth.onedrive.a").unwrap(), None);
            assert_eq!(v.get("oauth.dropbox.a").unwrap(), None);
            assert_eq!(v.get("oauth.google.a").unwrap().as_deref(), Some("t"));
        }
        let reads = inner.gets.load(Ordering::SeqCst) - before;
        // The vault, and the old-layout item of each missing key (a lookup that finds no item asks
        // nothing); then nothing for the rest of the 150 lookups.
        assert!(reads <= 3, "{reads} keychain reads for 150 lookups");
    }

    #[test]
    fn a_key_written_by_another_instance_is_found_after_one_reload() {
        let inner = Arc::new(Counting::default());
        let a = VaultStore::over(inner.clone()).with_miss_window(Duration::ZERO);
        let b = VaultStore::over(inner.clone());
        assert_eq!(a.get("late").unwrap(), None);
        b.set("late", "value").unwrap();
        assert_eq!(a.get("late").unwrap().as_deref(), Some("value"));
        // and writes by one never drop the keys of the other
        a.set("mine", "1").unwrap();
        b.set("yours", "2").unwrap();
        assert_eq!(a.get("mine").unwrap().as_deref(), Some("1"));
        assert_eq!(a.get("yours").unwrap().as_deref(), Some("2"));
    }

    #[test]
    fn an_item_of_the_old_layout_moves_into_the_vault_on_first_read() {
        let inner = Arc::new(Counting::default());
        inner.inner.set("oauth.google.a", "old-token").unwrap();
        let v = VaultStore::over(inner.clone());
        assert_eq!(
            v.get("oauth.google.a").unwrap().as_deref(),
            Some("old-token")
        );
        assert_eq!(
            keys(&inner),
            ["vault"],
            "the old item is gone, the vault holds it"
        );
        // a fresh vault over the same store still finds it (it really moved)
        let v2 = VaultStore::over(inner.clone());
        assert_eq!(
            v2.get("oauth.google.a").unwrap().as_deref(),
            Some("old-token")
        );
    }

    #[test]
    fn set_replaces_an_old_item_and_delete_removes_both() {
        let inner = Arc::new(Counting::default());
        inner.inner.set("x", "old").unwrap();
        let v = VaultStore::over(inner.clone());
        v.set("x", "new").unwrap();
        assert_eq!(keys(&inner), ["vault"]);
        assert_eq!(v.get("x").unwrap().as_deref(), Some("new"));
        v.delete("x").unwrap();
        assert_eq!(v.get("x").unwrap(), None);
        assert!(
            keys(&inner).is_empty(),
            "the last secret removes the vault item too"
        );
        v.delete("x").unwrap();
    }

    #[test]
    fn a_damaged_vault_is_an_error_and_is_not_overwritten() {
        let inner = Arc::new(Counting::default());
        inner.inner.set(VAULT_SLOT, "{ not json").unwrap();
        let v = VaultStore::over(inner.clone());
        let err = v.get("k").unwrap_err().to_string();
        assert!(err.contains("could not be read"), "{err}");
        assert!(v.set("k", "v").is_err());
        assert_eq!(
            inner.inner.get(VAULT_SLOT).unwrap().as_deref(),
            Some("{ not json")
        );
    }

    #[test]
    fn the_slot_name_is_reserved_and_debug_hides_values() {
        let v = VaultStore::over(Arc::new(Counting::default()));
        assert!(v.set(VAULT_SLOT, "x").is_err());
        v.set("k", "super-secret-value").unwrap();
        let dbg = format!("{v:?}");
        assert!(
            dbg.contains("\"k\"") && !dbg.contains("super-secret-value"),
            "{dbg}"
        );
    }

    #[test]
    fn concurrent_writers_keep_every_key() {
        let inner = Arc::new(Counting::default());
        let v = Arc::new(VaultStore::over(inner));
        let handles: Vec<_> = (0..16)
            .map(|i| {
                let v = v.clone();
                std::thread::spawn(move || v.set(&format!("key{i}"), &format!("value{i}")).unwrap())
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        for i in 0..16 {
            assert_eq!(
                v.get(&format!("key{i}")).unwrap(),
                Some(format!("value{i}"))
            );
        }
    }
}
