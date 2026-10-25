//! Connecting YouTube (PUB-1): the flow behind Settings → Destinations → Publish to → YouTube,
//! kept free of UI so it can be tested with fakes. It follows `cloud_connect` (the Drive flow)
//! and shares nothing with it but the building blocks in `cm-auth` and `cm-daemon`.
//!
//! The flow is human-only and runs in the app process: there is no action, MCP tool or CLI
//! command for it (agents only list the accounts). The user brings their own Google Cloud
//! client (BYO, specs/03-integrations.md):
//!
//! 1. [`validate_setup`] checks the pasted client ID and secret;
//! 2. [`connect`] opens the system browser through `cm_auth::AuthFlow` (PKCE, loopback redirect)
//!    with `ProviderConfig::google_youtube()`, stores the tokens and the credentials in the
//!    secret store (the OS keychain in the app), asks YouTube which channel this is
//!    (`channels.list?mine=true`) and writes the account to `publish_accounts.json`
//!    (`cm_publish::accounts`). The file holds no secret;
//! 3. [`test_connection`] renews the token and lists the channel again; [`account_state`] says
//!    whether the card needs "Reconnect"; [`disconnect_and_revoke`] first asks Google to revoke
//!    the stored sign-in (best effort, PUB-1.AC5), then [`disconnect`] forgets the account and
//!    its secrets, whether or not Google could be reached. No video is touched.
//!
//! Everything outside is injected through [`Env`]: browser, sign-in server, YouTube's API,
//! secret store.

use crate::cloud_connect::ReconnectWhy;
use std::sync::Arc;
use std::time::Duration;

use cm_auth::credentials::{forget_credentials, load_credentials, save_credentials};
use cm_auth::{
    revoke_account, AuthFlow, BrowserOpener, ClientCredentials, HttpClient, ProviderConfig,
    RevokeOutcome, SecretStore, SystemBrowser, TokenManager, UreqClient,
};
use cm_core::CmError;
use cm_daemon::storage_cloud::{AccountTokens, KeychainTokens};
use cm_i18n::{tr, tr_ctx};
use cm_publish::accounts::{PublishAccount, PublishAccountStore};
use cm_publish::channel::{my_channel, Channel};
use cm_publish::{Privacy, StorageHttp, UreqHttp};

/// How long the browser sign-in may take before the flow gives up.
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);
/// The platform key of a YouTube account in `publish_accounts.json`.
pub const PLATFORM: &str = "youtube";
const CLIENT_ID_SUFFIX: &str = ".apps.googleusercontent.com";

/// The text of the guided setup. The steps and notes are built when drawn, in the current language.
pub struct SetupGuide {
    pub name: &'static str,
    pub client_id_hint: &'static str,
}

impl SetupGuide {
    /// Numbered steps.
    pub fn steps(&self) -> Vec<String> {
        vec![
            tr!("Open the Google Cloud Console (console.cloud.google.com) and create a project, or pick one you already have. A project you made for Google Drive works."),
            tr!("APIs & Services \u{2192} Library: search for \u{201c}YouTube Data API v3\u{201d} and enable it."),
            tr!("APIs & Services \u{2192} OAuth consent screen: choose External, give the app a name, and add your own Google address as a test user. Add the YouTube scopes \u{201c}youtube.upload\u{201d} and \u{201c}youtube\u{201d} if the console asks which ones the app uses."),
            tr!("APIs & Services \u{2192} Credentials \u{2192} Create credentials \u{2192} OAuth client ID. Set the application type to \u{201c}Desktop app\u{201d}. Make a new client for YouTube; do not reuse the Drive one."),
            tr!("Copy the client ID and the client secret Google shows, and paste them below."),
        ]
    }

    /// Cautions shown under the steps: the unverified-app screen, test users, the daily limit.
    pub fn notes(&self) -> Vec<String> {
        vec![
            tr!("Google will say \u{201c}Google hasn\u{2019}t verified this app\u{201d} when you sign in. That is your own app, so choose Advanced, then \u{201c}Go to \u{2026} (unsafe)\u{201d}. An app in Testing mode only lets the test users you listed sign in (up to 100), and Google ends the sign-in after 7 days; choose \u{201c}Publish app\u{201d} on the consent screen to keep it signed in."),
            tr!("Each Google Cloud project may upload to YouTube about 100 times a day. YouTube can also keep videos from an unverified project private; CaptureMann tells you when that happens and never changes a video\u{2019}s visibility afterwards."),
        ]
    }
}

pub const YOUTUBE_GUIDE: SetupGuide = SetupGuide {
    name: "YouTube", // i18n-ignore: product name
    client_id_hint: "123456789-abc.apps.googleusercontent.com",
};

/// The Google client ID/secret the user pasted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetupForm {
    pub client_id: String,
    pub client_secret: String,
}

pub use crate::cloud_connect::{FieldError, SetupErrors};

/// Checks the pasted values without any network: both trimmed, no spaces inside, the ID in
/// Google's shape, a secret (Google desktop clients need one).
pub fn validate_setup(form: &SetupForm) -> Result<ClientCredentials, SetupErrors> {
    let mut errors = SetupErrors::default();
    let id = form.client_id.trim();
    if id.is_empty() {
        errors.client_id = Some(FieldError::IdMissing(cm_storage::Kind::GoogleDrive));
    } else if id.chars().any(char::is_whitespace) {
        errors.client_id = Some(FieldError::IdSpaces);
    } else if !id.ends_with(CLIENT_ID_SUFFIX) {
        errors.client_id = Some(FieldError::NotGoogleId(CLIENT_ID_SUFFIX));
    }
    let secret = form.client_secret.trim();
    if secret.is_empty() {
        errors.client_secret = Some(FieldError::SecretMissing);
    } else if secret.chars().any(char::is_whitespace) {
        errors.client_secret = Some(FieldError::SecretSpaces);
    }
    if errors != SetupErrors::default() {
        return Err(errors);
    }
    Ok(ClientCredentials {
        client_id: id.to_owned(),
        client_secret: Some(secret.to_owned()),
    })
}

/// Everything the flow talks to. The product wires the real ones with [`Env::system`].
pub struct Env {
    pub secrets: Arc<dyn SecretStore>,
    /// The token endpoint.
    pub auth_http: Arc<dyn HttpClient>,
    pub browser: Arc<dyn BrowserOpener>,
    /// YouTube's API.
    pub http: Arc<dyn StorageHttp>,
    /// The daemon's token source; the flow clears its cache after connecting.
    pub tokens: Arc<KeychainTokens>,
    pub store: PublishAccountStore,
    pub timeout: Duration,
}

impl Env {
    /// The OS keychain, the system browser and the real network, writing `publish_accounts.json`
    /// in `store`. Shares the process's token cache with the daemon.
    pub fn system(store: PublishAccountStore) -> Env {
        Env {
            secrets: Arc::new(cm_auth::VaultStore::system(
                cm_daemon::storage_cloud::KEYCHAIN_SERVICE,
            )),
            auth_http: Arc::new(UreqClient::new()),
            browser: Arc::new(SystemBrowser),
            http: Arc::new(UreqHttp::new()),
            tokens: KeychainTokens::system(),
            store,
            timeout: SIGN_IN_TIMEOUT,
        }
    }
}

/// Where the flow is, for the setup screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Waiting for the user in the browser.
    Browser,
    /// Asking which channel signed in.
    Channel,
    /// Writing the account.
    Saving,
}

impl Phase {
    pub fn label(self) -> String {
        match self {
            Phase::Browser => tr!("Waiting for you to sign in in the browser\u{2026}"),
            Phase::Channel => tr!("Looking up your channel\u{2026}"),
            Phase::Saving => tr!("Saving the account\u{2026}"),
        }
    }
}

/// A new account, or a new sign-in for one that exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    New,
    Reconnect { account_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connected {
    pub account_id: String,
    pub channel_name: String,
    pub reconnected: bool,
    /// A reconnect signed in with another channel than before.
    pub channel_changed: bool,
}

/// `ProviderConfig` id of the keychain entries (`oauth.google.<account>`).
fn provider_id() -> &'static str {
    ProviderConfig::google_youtube().id
}

/// A fresh account key such as `youtube-k3x9a2bq`: lower-case letters and digits.
pub fn new_account_key() -> Result<String, CmError> {
    let mut suffix = String::new();
    for _ in 0..8 {
        if suffix.len() >= 8 {
            break;
        }
        suffix.extend(
            cm_auth::pkce::random_token(12)?
                .chars()
                .filter(char::is_ascii_alphanumeric)
                .map(|c| c.to_ascii_lowercase()),
        );
    }
    suffix.truncate(8);
    if suffix.len() < 8 {
        return Err(CmError::internal("could not make an account key"));
    }
    Ok(format!("{PLATFORM}-{suffix}"))
}

/// Runs the whole connect flow. Nothing is stored until the browser sign-in succeeded. A new
/// connection that fails after that leaves no secret behind; a reconnect keeps the new sign-in.
pub async fn connect(
    env: &Env,
    creds: ClientCredentials,
    target: &Target,
    phase: &(dyn Fn(Phase) + Send + Sync),
) -> Result<Connected, CmError> {
    let reconnecting = match target {
        Target::New => None,
        Target::Reconnect { account_id } => {
            let a = env.store.get(account_id)?;
            if a.platform != PLATFORM {
                return Err(CmError::invalid_input(tr!(
                    "\u{201c}{name}\u{201d} is not a YouTube account.",
                    name = a.name
                )));
            }
            Some(a)
        }
    };
    let config = ProviderConfig::google_youtube();
    phase(Phase::Browser);
    let tokens = AuthFlow {
        provider: &config,
        creds: &creds,
        http: env.auth_http.as_ref(),
        browser: env.browser.as_ref(),
    }
    .authorize(env.timeout)
    .await?;

    let key = match &reconnecting {
        Some(a) => a.id.clone(),
        None => new_account_key()?,
    };
    let manager = TokenManager::new(
        config.clone(),
        creds.clone(),
        env.auth_http.clone(),
        env.secrets.clone(),
    );
    let stored = save_credentials(env.secrets.as_ref(), config.id, &key, &creds)
        .and_then(|()| manager.save(&key, &tokens));
    env.tokens.invalidate(&key);
    let result = match stored {
        Ok(()) => finish(env, &manager, &key, reconnecting.as_ref(), phase).await,
        Err(e) => Err(e),
    };
    if result.is_err() && reconnecting.is_none() {
        // A half-connected account would only confuse the next try.
        let _ = manager.forget(&key);
        let _ = forget_credentials(env.secrets.as_ref(), config.id, &key);
        env.tokens.invalidate(&key);
    }
    result
}

async fn finish(
    env: &Env,
    manager: &TokenManager,
    key: &str,
    reconnecting: Option<&PublishAccount>,
    phase: &(dyn Fn(Phase) + Send + Sync),
) -> Result<Connected, CmError> {
    phase(Phase::Channel);
    let channel = my_channel(env.http.as_ref(), env.tokens.source(key).as_ref()).await?;
    // Remember who it is next to the tokens.
    if let Some(mut t) = manager.load(key)? {
        t.account_label = Some(channel.title.clone());
        manager.save(key, &t)?;
    }
    phase(Phase::Saving);
    let channel_changed = reconnecting.is_some_and(|a| a.account_id != channel.id);
    let account = PublishAccount {
        id: key.to_owned(),
        platform: PLATFORM.to_owned(),
        name: channel.title.clone(),
        account_id: channel.id.clone(),
        // Kept across a reconnect; a new account starts at the platform's default.
        default_privacy: reconnecting
            .map(|a| a.default_privacy)
            .unwrap_or(cm_publish::PlatformRules::youtube().default_privacy),
    };
    env.store.upsert(account)?;
    Ok(Connected {
        account_id: key.to_owned(),
        channel_name: channel.title,
        reconnected: reconnecting.is_some(),
        channel_changed,
    })
}

/// "Test connection": renews the token and lists the channel again (read-only).
pub async fn test_connection(env: &Env, account_id: &str) -> Result<Channel, CmError> {
    let account = env.store.get(account_id)?;
    let source = env.tokens.source(&account.id);
    // A token the server "rejected" is renewed even if it has not expired, so a revoked refresh
    // token or a deleted client shows up here and not at the first upload.
    cm_publish::TokenSource::access_token(source.as_ref(), Some("renew")).await?;
    my_channel(env.http.as_ref(), source.as_ref()).await
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountState {
    Connected,
    /// The sign-in is gone or was refused: the card shows "Reconnect" with this reason.
    Reconnect(ReconnectWhy),
}

/// Whether an account can publish now, without the network: its tokens and credentials are in
/// the secret store. Reads the keychain, so call it off the UI thread.
pub fn account_state(secrets: &dyn SecretStore, account: &PublishAccount) -> AccountState {
    let token_key = format!("oauth.{}.{}", provider_id(), account.id);
    match (
        secrets.get(&token_key),
        load_credentials(secrets, provider_id(), &account.id),
    ) {
        (Ok(Some(_)), Ok(Some(_))) => AccountState::Connected,
        (Ok(None), _) | (_, Ok(None)) => AccountState::Reconnect(ReconnectWhy::NotSignedIn),
        (Err(e), _) | (_, Err(e)) => AccountState::Reconnect(ReconnectWhy::Os(e.to_string())),
    }
}

/// Changes the privacy the publish sheet preselects for the account.
pub fn set_default_privacy(
    env: &Env,
    account_id: &str,
    privacy: Privacy,
) -> Result<PublishAccount, CmError> {
    let mut a = env.store.get(account_id)?;
    a.default_privacy = privacy;
    env.store.upsert(a.clone())?;
    Ok(a)
}

/// Makes the account the one the publish sheet starts with.
pub fn make_default(env: &Env, account_id: &str) -> Result<(), CmError> {
    env.store.make_default(account_id)
}

/// Asks Google to revoke the account's sign-in, then forgets the account and its secrets. A
/// revoke that fails (offline, refused) does not stop the local sign-out: the outcome says what
/// happened, and [`sign_out_message`] puts it in words. An unknown account is `not_found` and
/// nothing is sent.
pub async fn disconnect_and_revoke(env: &Env, account_id: &str) -> Result<RevokeOutcome, CmError> {
    let account = env.store.get(account_id)?;
    let outcome = revoke_account(
        env.auth_http.as_ref(),
        env.secrets.as_ref(),
        &ProviderConfig::google_youtube(),
        &account.id,
    )
    .await;
    disconnect(env, account_id)?;
    Ok(outcome)
}

/// What the user reads after "Disconnect".
pub fn sign_out_message(name: &str, outcome: &RevokeOutcome) -> String {
    match outcome {
        RevokeOutcome::Revoked => tr!(
            "Disconnected \u{201c}{name}\u{201d}. Google was told to remove CaptureMann\u{2019}s access.",
            name = name
        ),
        RevokeOutcome::NotAttempted => tr!("Disconnected \u{201c}{name}\u{201d}.", name = name),
        RevokeOutcome::Failed(_) => tr!(
            "\u{201c}{name}\u{201d} is signed out here; Google could not be reached to revoke access, you can remove it at myaccount.google.com/permissions.",
            name = name
        ),
    }
}

/// Removes the account and its tokens and credentials from the keychain, without telling
/// Google (see [`disconnect_and_revoke`]). No video is touched.
pub fn disconnect(env: &Env, account_id: &str) -> Result<(), CmError> {
    let gone = env
        .store
        .remove(account_id)?
        .ok_or_else(|| CmError::not_found(tr!("That account no longer exists.")))?;
    env.secrets
        .delete(&format!("oauth.{}.{}", provider_id(), gone.id))?;
    forget_credentials(env.secrets.as_ref(), provider_id(), &gone.id)?;
    env.tokens.invalidate(&gone.id);
    Ok(())
}

/// The detail line of the YouTube card: the channel and the default visibility.
pub fn card_detail(account: &PublishAccount) -> String {
    tr!(
        "{channel} \u{b7} default visibility: {visibility}",
        channel = account.name,
        visibility = privacy_label(account.default_privacy)
    )
}

pub fn privacy_label(p: Privacy) -> String {
    match p {
        Privacy::Private => tr_ctx!("visibility", "Private"),
        Privacy::Unlisted => tr_ctx!("visibility", "Unlisted"),
        Privacy::Public => tr_ctx!("visibility", "Public"),
    }
}

#[cfg(test)]
mod tests;
