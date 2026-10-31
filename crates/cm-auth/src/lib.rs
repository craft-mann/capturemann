//! OAuth for a desktop app (M6; specs/03-integrations.md "Desktop OAuth flow", specs/01 "Secrets").
//!
//! * [`secret`]: the [`SecretStore`] trait, the OS-keychain [`KeyringStore`], the single-item
//!   [`VaultStore`] the app uses (one keychain item for all secrets, so one prompt) and the test
//!   [`MemoryStore`]. Config files hold only key names, never secrets.
//! * [`pkce`]: RFC 7636 verifier and S256 challenge.
//! * [`provider`]: Google, Microsoft and Dropbox endpoints and scopes; the client id (and secret) are
//!   the user's own and are never shipped.
//! * [`flow`]: the authorization-code flow: authorization URL, system browser ([`BrowserOpener`]),
//!   the one-shot loopback listener ([`loopback`]) and the code exchange through an [`HttpClient`].
//! * [`token`]: one JSON secret per account and [`TokenManager::access_token`], which refreshes on
//!   demand and maps failures to the shared error codes.
//!
//! Tokens, codes and client secrets are never written to logs or error messages; the types that hold
//! them print `[redacted]` in `Debug`.

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod browser;
pub mod credentials;
pub mod file_store;
pub mod flow;
pub mod http;
pub mod loopback;
pub mod pkce;
pub mod provider;
pub mod revoke;
pub mod secret;
pub mod token;

pub use browser::{BrowserOpener, SystemBrowser};
pub use file_store::{backend_from_env, choose_backend, Backend, FileStore};
pub use flow::{build_authorization_url, AuthFlow};
pub use http::{HttpClient, HttpError, HttpResponse, UreqClient};
pub use loopback::LoopbackListener;
pub use provider::{ClientCredentials, ProviderConfig};
pub use revoke::{revoke_account, RevokeOutcome};
pub use secret::{KeyringStore, MemoryStore, SecretStore, VaultStore, VAULT_SLOT};
pub use token::{TokenManager, TokenSet};

/// Text from a server, scrubbed and shortened for an error message.
pub(crate) fn safe_text(s: &str) -> String {
    let scrubbed = cm_jobs::scrub_secrets(s);
    scrubbed.chars().take(200).collect()
}
