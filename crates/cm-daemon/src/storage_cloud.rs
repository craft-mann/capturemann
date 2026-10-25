//! Cloud providers for the `upload` job (STO-2): picks the provider from the destination kind and
//! gives it the account's token source.
//!
//! The product uses [`KeychainTokens`] (`CloudBackend::system()`): tokens and the user's own client
//! credentials come from the OS keychain through `cm-auth`, refreshed on demand. The default
//! [`CloudBackend`] has [`NoAccounts`], whose sources answer `auth_expired`, so tests never touch
//! the keychain or the network. Everything here is injectable (a fake HTTP server, scripted
//! tokens, an in-memory secret store).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use cm_auth::{
    credentials::load_credentials, HttpClient, ProviderConfig, SecretStore, TokenManager,
    UreqClient, VaultStore,
};
use cm_core::{CmError, ErrorCode};
use cm_storage::{
    Destination, DriveConfig, DropboxConfig, DropboxProvider, GoogleDriveProvider, Kind,
    OneDriveConfig, OneDriveProvider, StorageHttp, StorageProvider, TokenSource, UreqHttp,
};

/// Where the access token of a connected account comes from.
pub trait AccountTokens: Send + Sync {
    /// The token source of `account` (the key stored in the destination).
    fn source(&self, account: &str) -> Arc<dyn TokenSource>;

    /// The OAuth client ID the account was connected with (the user's own app; not a secret), or
    /// `None` when it is not known. Used to count YouTube's daily uploads per Google Cloud
    /// project: two accounts that share one client share the 100 uploads a day.
    fn client_id(&self, _account: &str) -> Option<String> {
        None
    }
}

/// No account is connected: every source answers `auth_expired`.
pub struct NoAccounts;

struct NotConnected;

#[async_trait]
impl TokenSource for NotConnected {
    async fn access_token(&self, _rejected: Option<&str>) -> Result<String, CmError> {
        Err(CmError::auth_expired(
            "This account is not connected. Connect it again in Settings → Destinations.",
        ))
    }
}

impl AccountTokens for NoAccounts {
    fn source(&self, _account: &str) -> Arc<dyn TokenSource> {
        Arc::new(NotConnected)
    }
}

/// The keychain service the app's secrets live under (the bundle id).
pub const KEYCHAIN_SERVICE: &str = "dev.capturemann.app";

/// Account tokens from the secret store (the OS keychain in the product), by the account key a
/// destination holds. The provider of an account is found from the token it has stored, the
/// client credentials are the user's own (`cm_auth::credentials`). One [`TokenManager`] per
/// account serialises refreshes.
pub struct KeychainTokens {
    store: Arc<dyn SecretStore>,
    http: Arc<dyn HttpClient>,
    managers: Mutex<HashMap<String, Arc<TokenManager>>>,
    client_ids: Mutex<HashMap<String, Option<String>>>,
}

impl KeychainTokens {
    pub fn new(store: Arc<dyn SecretStore>, http: Arc<dyn HttpClient>) -> Arc<Self> {
        Arc::new(KeychainTokens {
            store,
            http,
            managers: Mutex::new(HashMap::new()),
            client_ids: Mutex::new(HashMap::new()),
        })
    }

    /// The OS keychain and the system TLS stack. One instance per process, so the connect flow in
    /// the app and the daemon's uploads share the cache that [`invalidate`](Self::invalidate)
    /// clears.
    pub fn system() -> Arc<Self> {
        static SYSTEM: std::sync::OnceLock<Arc<KeychainTokens>> = std::sync::OnceLock::new();
        SYSTEM
            .get_or_init(|| {
                Self::new(
                    Arc::new(VaultStore::system(KEYCHAIN_SERVICE)),
                    Arc::new(UreqClient::new()),
                )
            })
            .clone()
    }

    /// Forgets the cached manager of `account` (call after it was connected again or removed, so
    /// new credentials are read).
    pub fn invalidate(&self, account: &str) {
        self.client_ids
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(account);
        self.managers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(account);
    }

    /// The client ID stored for `account` (Google first: the publishing accounts are Google
    /// ones). Remembered until [`invalidate`](Self::invalidate), so a list of accounts does not
    /// read the keychain on every call. A missing or unreadable entry is `None`.
    pub fn client_id_of(&self, account: &str) -> Option<String> {
        if let Some(known) = self
            .client_ids
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(account)
        {
            return known.clone();
        }
        let id = Self::providers().iter().find_map(|p| {
            load_credentials(self.store.as_ref(), p.id, account)
                .ok()
                .flatten()
                .map(|c| c.client_id)
        });
        self.client_ids
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(account.to_owned(), id.clone());
        id
    }

    /// The providers an account can belong to, in the order they are tried.
    fn providers() -> [ProviderConfig; 3] {
        [
            ProviderConfig::google_drive(),
            ProviderConfig::microsoft(),
            ProviderConfig::dropbox(),
        ]
    }

    fn manager(&self, account: &str) -> Result<Arc<TokenManager>, CmError> {
        if let Some(m) = self
            .managers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(account)
        {
            return Ok(m.clone());
        }
        let not_connected = || {
            CmError::auth_expired(
                "This account is not connected. Connect it again in Settings → Destinations.",
            )
        };
        let mut found = None;
        for provider in Self::providers() {
            let key = format!("oauth.{}.{account}", provider.id);
            if self.store.get(&key)?.is_some() {
                found = Some(provider);
                break;
            }
        }
        let provider = found.ok_or_else(not_connected)?;
        let creds = load_credentials(self.store.as_ref(), provider.id, account)?.ok_or_else(|| {
            CmError::auth_expired(
                "The app credentials of this account are missing. Connect it again in Settings → Destinations.",
            )
        })?;
        let m = Arc::new(TokenManager::new(
            provider,
            creds,
            self.http.clone(),
            self.store.clone(),
        ));
        self.managers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(account.to_owned(), m.clone());
        Ok(m)
    }

    /// A valid token. With `rejected` (the server answered 401) the token is renewed even though
    /// it has not expired. A refresh the provider refuses (revoked, or the client was deleted)
    /// is `auth_expired`; a network failure stays `network`, which the job retries.
    async fn token(&self, account: &str, rejected: Option<&str>) -> Result<String, CmError> {
        let m = self.manager(account)?;
        let result = match rejected {
            None => m.access_token(account).await,
            Some(_) => m.force_refresh(account).await,
        };
        match result {
            Err(e) if e.code() == ErrorCode::PlatformRejected => {
                self.invalidate(account);
                Err(CmError::auth_expired(
                    "The provider refused to renew the connection. Connect the account again and check the client ID.",
                ))
            }
            Err(e) if e.code() == ErrorCode::AuthExpired => {
                // A reconnect (maybe by another process) may bring new credentials.
                self.invalidate(account);
                Err(e)
            }
            other => other,
        }
    }
}

struct AccountSource {
    tokens: Arc<KeychainTokens>,
    account: String,
}

#[async_trait]
impl TokenSource for AccountSource {
    async fn access_token(&self, rejected: Option<&str>) -> Result<String, CmError> {
        self.tokens.token(&self.account, rejected).await
    }
}

/// `Arc<KeychainTokens>` is the [`AccountTokens`] of the product.
impl AccountTokens for Arc<KeychainTokens> {
    fn source(&self, account: &str) -> Arc<dyn TokenSource> {
        Arc::new(AccountSource {
            tokens: self.clone(),
            account: account.to_owned(),
        })
    }

    fn client_id(&self, account: &str) -> Option<String> {
        self.client_id_of(account)
    }
}

/// The pieces a cloud provider needs.
#[derive(Clone)]
pub struct CloudBackend {
    pub tokens: Arc<dyn AccountTokens>,
    pub http: Arc<dyn StorageHttp>,
}

impl Default for CloudBackend {
    fn default() -> Self {
        CloudBackend {
            tokens: Arc::new(NoAccounts),
            http: Arc::new(UreqHttp::new()),
        }
    }
}

impl CloudBackend {
    /// The product's backend: tokens from the OS keychain, real HTTP. Tests use
    /// [`CloudBackend::default`] or their own parts.
    pub fn system() -> Self {
        Self::with_tokens(KeychainTokens::system())
    }

    /// The real HTTP stack with these token sources (the app keeps a handle on its
    /// [`KeychainTokens`] to call `invalidate` after connecting).
    pub fn with_tokens(tokens: Arc<KeychainTokens>) -> Self {
        CloudBackend {
            tokens: Arc::new(tokens),
            http: Arc::new(UreqHttp::new()),
        }
    }

    /// The provider of a cloud destination.
    pub fn provider(&self, dest: &Destination) -> Result<Arc<dyn StorageProvider>, CmError> {
        let account = dest.account.as_ref().ok_or_else(|| {
            CmError::invalid_input(format!(
                "destination `{}` has no account; connect one in Settings → Destinations",
                dest.id
            ))
        })?;
        match dest.kind {
            Kind::GoogleDrive => Ok(Arc::new(GoogleDriveProvider::new(
                self.http.clone(),
                self.tokens.source(&account.account),
                DriveConfig {
                    root_folder_name: account.folder_name.clone(),
                    root_folder_id: account.folder_id.clone(),
                    link_mode: account.link_mode,
                    ..DriveConfig::default()
                },
            ))),
            Kind::OneDrive => Ok(Arc::new(OneDriveProvider::new(
                self.http.clone(),
                self.tokens.source(&account.account),
                OneDriveConfig {
                    root_folder_name: account.folder_name.clone(),
                    root_folder_id: account.folder_id.clone(),
                    link_mode: account.link_mode,
                    ..OneDriveConfig::default()
                },
            ))),
            Kind::Dropbox => Ok(Arc::new(DropboxProvider::new(
                self.http.clone(),
                self.tokens.source(&account.account),
                DropboxConfig {
                    root_folder_name: account.folder_name.clone(),
                    app_folder: account.app_folder,
                    link_mode: account.link_mode,
                    ..DropboxConfig::default()
                },
            ))),
            Kind::Local => Err(CmError::internal(
                "a local destination has no cloud provider",
            )),
        }
    }
}
