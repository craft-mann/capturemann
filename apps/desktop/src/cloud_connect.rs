//! Connecting a cloud drive (STO-2): the flow behind Settings → Destinations → Connect, kept free
//! of UI so it can be tested with fakes.
//!
//! The flow is human-only and runs in the app process: there is no action, MCP tool or CLI
//! command for it. The user brings their own OAuth client (BYO, specs/03-integrations.md):
//!
//! 1. [`validate_setup`] checks the pasted client ID (and secret where the provider needs one);
//! 2. [`connect`] opens the system browser through `cm_auth::AuthFlow` (PKCE, loopback redirect),
//!    stores the tokens and the credentials in the secret store (the OS keychain in the app), asks
//!    the provider which account this is and creates the "CaptureMann" folder, then writes the
//!    destination through `DestinationStore`. Config files hold only the account key.
//! 3. [`test_connection`] uploads and deletes a 1 KB file; [`account_state`] says whether the
//!    destination needs "Reconnect"; [`disconnect`] forgets the destination and its secrets
//!    (nothing is revoked at the provider).
//!
//! Every outside thing is injected through [`Env`]: browser, sign-in server, Drive, secret store.
//! A provider is one [`ProviderInfo`] row plus its branches in [`verify`], [`test_connection`] and
//! [`store_provider`]: Google Drive, OneDrive (Microsoft Entra app, public client, no secret) and
//! Dropbox (scoped app, PKCE, no secret, "full Dropbox" or "App folder" access).

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use cm_auth::credentials::{forget_credentials, load_credentials, save_credentials};
use cm_auth::{
    revoke_account, AuthFlow, BrowserOpener, ClientCredentials, HttpClient, ProviderConfig,
    RevokeOutcome, SecretStore, SystemBrowser, TokenManager, UreqClient,
};
use cm_core::CmError;
use cm_daemon::storage_cloud::{AccountTokens, KeychainTokens};
use cm_i18n::tr;
use cm_storage::{
    AutoUpload, CloudAccount, Destination, DestinationStore, DriveConfig, DropboxConfig,
    DropboxProvider, GoogleDriveProvider, Kind, LinkMode, OneDriveConfig, OneDriveProvider,
    StorageHttp, UreqHttp, What, DEFAULT_FOLDER_PATTERN,
};
use serde_json::Value;

use crate::destinations as rules;

/// The folder created in the user's drive and named in the destination.
pub const FOLDER_NAME: &str = "CaptureMann";
/// How long the browser sign-in may take before the flow gives up.
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

/// A cloud provider the app can connect.
#[derive(Clone, Copy)]
pub struct ProviderInfo {
    /// Stable key, also the prefix of account keys.
    pub key: &'static str,
    /// The name in the "Save to" list and in the default destination name.
    pub name: &'static str,
    pub kind: Kind,
    pub config: fn() -> ProviderConfig,
    /// What the client ID looks like, for the field's hint and its check.
    pub client_id_hint: &'static str,
    id_ok: fn(&str) -> bool,
    /// Dropbox: the app was registered with "App folder" or "Full Dropbox" access and the user
    /// says which (stored in `CloudAccount.app_folder`).
    pub access_choice: bool,
}

impl ProviderInfo {
    /// Whether the provider's token endpoint needs a client secret (Google desktop clients do).
    pub fn needs_secret(&self) -> bool {
        (self.config)().client_secret_required
    }
}

impl ProviderInfo {
    /// What the provider calls the client ID ("Client ID", "Application (client) ID", "App key").
    pub fn id_label(&self) -> String {
        match self.kind {
            Kind::GoogleDrive => tr!("Client ID"),
            Kind::OneDrive => tr!("Application (client) ID"),
            Kind::Dropbox | Kind::Local => tr!("App key"),
        }
    }

    /// The label of the secret field (empty for providers that need none).
    pub fn secret_label(&self) -> String {
        match self.kind {
            Kind::GoogleDrive => tr!("Client secret"),
            Kind::OneDrive | Kind::Dropbox | Kind::Local => String::new(),
        }
    }

    /// The steps to create the user's own client, shown on the setup screen.
    pub fn steps(&self) -> Vec<String> {
        match self.kind {
            Kind::GoogleDrive => vec![
                tr!("Open the Google Cloud Console (console.cloud.google.com) and create a project, or pick one you already have."),
                tr!("APIs & Services \u{2192} Library: search for \u{201c}Google Drive API\u{201d} and enable it."),
                tr!("APIs & Services \u{2192} OAuth consent screen: choose External, give the app a name, and add your own Google address as a test user."),
                tr!("APIs & Services \u{2192} Credentials \u{2192} Create credentials \u{2192} OAuth client ID. Set the application type to \u{201c}Desktop app\u{201d}."),
                tr!("Copy the client ID and the client secret Google shows, and paste them below."),
            ],
            Kind::OneDrive => vec![
                tr!("Open the Microsoft Entra admin center (entra.microsoft.com) and sign in with the account that owns the OneDrive (a work or school account, or a personal Microsoft account). Go to Identity \u{2192} Applications \u{2192} App registrations \u{2192} New registration."),
                tr!("Give it a name such as \u{201c}CaptureMann\u{201d}. Under Supported account types choose \u{201c}Accounts in any organizational directory and personal Microsoft accounts\u{201d} so both kinds of account can sign in."),
                tr!("Under Redirect URI choose the platform \u{201c}Public client/native (mobile & desktop)\u{201d} and enter http://127.0.0.1 (a loopback address; the port is ignored). Select Register."),
                tr!("Open Authentication and make sure \u{201c}Allow public client flows\u{201d} is Yes. No client secret is needed; do not create one."),
                tr!("Open API permissions \u{2192} Add a permission \u{2192} Microsoft Graph \u{2192} Delegated permissions, and add Files.ReadWrite and offline_access. No admin consent is needed."),
                tr!("On the Overview page copy the \u{201c}Application (client) ID\u{201d} and paste it below."),
            ],
            Kind::Dropbox | Kind::Local => vec![
                tr!("Open the Dropbox App Console (dropbox.com/developers/apps) and choose Create app."),
                tr!("Choose \u{201c}Scoped access\u{201d}, then the access type: \u{201c}App folder\u{201d} (Dropbox gives CaptureMann its own folder under Apps) or \u{201c}Full Dropbox\u{201d}. Name the app and create it."),
                tr!("On the Permissions tab tick account_info.read, files.metadata.read, files.content.write, sharing.read and sharing.write, then choose Submit."),
                tr!("On the Settings tab, under Redirect URIs, add http://127.0.0.1/callback (a loopback address; the port is ignored) and choose Add. Copy the \u{201c}App key\u{201d}. The app secret is not needed."),
                tr!("Paste the App key below and say which access type you chose."),
            ],
        }
    }

    /// A caution shown under the steps.
    pub fn note(&self) -> String {
        match self.kind {
            Kind::GoogleDrive => tr!(
                "CaptureMann asks only for access to the files it creates (the drive.file scope). While the consent screen is in \u{201c}Testing\u{201d}, Google ends the sign-in after 7 days; choose \u{201c}Publish app\u{201d} on the consent screen to keep it signed in (this scope needs no review)."
            ),
            Kind::OneDrive => tr!(
                "A personal Microsoft account can make \u{201c}anyone with the link\u{201d} links but not \u{201c}people in my organization\u{201d} links. A work or school account can make both, unless its administrator forbids public links; then CaptureMann makes the organization link instead and says so. If sign-in later says the redirect does not match, add http://127.0.0.1/callback as a second redirect URI."
            ),
            Kind::Dropbox | Kind::Local => tr!(
                "Dropbox shows a development app to its owner only, which is all CaptureMann needs. Set the permissions before you sign in: Dropbox ties the permissions to the sign-in, so changing them later means signing in again. The access type cannot be changed after the app is created."
            ),
        }
    }

    /// After "said no": what to check in the user's own app.
    pub fn refusal_hint(&self) -> String {
        match self.kind {
            Kind::GoogleDrive => tr!(
                "Check the client ID and secret, and that the Google Drive API is enabled in your project."
            ),
            Kind::OneDrive => tr!(
                "Check the application (client) ID, that \u{201c}Allow public client flows\u{201d} is Yes, and that http://127.0.0.1 is a redirect URI of the app."
            ),
            Kind::Dropbox | Kind::Local => tr!(
                "Check the app key, that the permissions were submitted on the Permissions tab before signing in, and that http://127.0.0.1/callback is a redirect URI of the app."
            ),
        }
    }
}

fn google_id_ok(id: &str) -> bool {
    id.ends_with(".apps.googleusercontent.com")
}

/// A Microsoft application (client) ID is a GUID: 8-4-4-4-12 hexadecimal digits.
fn guid_ok(id: &str) -> bool {
    let groups: Vec<&str> = id.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_hexdigit()))
}

/// A Dropbox app key is a run of letters and digits (15 today).
fn app_key_ok(id: &str) -> bool {
    (8..=40).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric())
}

pub const GOOGLE_DRIVE: ProviderInfo = ProviderInfo {
    key: "gdrive",
    name: "Google Drive",
    kind: Kind::GoogleDrive,
    config: ProviderConfig::google_drive,
    client_id_hint: "123456789-abc.apps.googleusercontent.com",
    id_ok: google_id_ok,
    access_choice: false,
};

pub const ONEDRIVE: ProviderInfo = ProviderInfo {
    key: "onedrive",
    name: "OneDrive",
    kind: Kind::OneDrive,
    config: ProviderConfig::microsoft,
    client_id_hint: "11111111-2222-3333-4444-555555555555",
    id_ok: guid_ok,
    access_choice: false,
};

pub const DROPBOX: ProviderInfo = ProviderInfo {
    key: "dropbox",
    name: "Dropbox",
    kind: Kind::Dropbox,
    config: ProviderConfig::dropbox,
    client_id_hint: "abcd1234efgh567",
    id_ok: app_key_ok,
    access_choice: true,
};

/// Providers with a connect flow.
pub const PROVIDERS: [ProviderInfo; 3] = [GOOGLE_DRIVE, ONEDRIVE, DROPBOX];

/// The providers the app offers in v0.1 (owner decision Oct 23, 2026): Google Drive only. OneDrive
/// and Dropbox are built and tested with fakes but their rows say "Coming soon" until user
/// feedback or a contribution asks for them; to offer one, add its name here.
pub const OFFERED: [&str; 1] = ["Google Drive"]; // i18n-ignore: product name

/// Whether the app offers the provider named `name` (see [`OFFERED`]).
pub fn is_offered(name: &str) -> bool {
    OFFERED.contains(&name)
}

/// The provider behind a row of the "Save to" list, by its name; `None` while it is not built.
pub fn provider_named(name: &str) -> Option<&'static ProviderInfo> {
    PROVIDERS.iter().find(|p| p.name == name)
}

/// The provider of a stored destination.
pub fn provider_of(dest: &Destination) -> Option<&'static ProviderInfo> {
    PROVIDERS.iter().find(|p| p.kind == dest.kind)
}

// ---- the setup screen --------------------------------------------------------------------------

/// What the user pasted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetupForm {
    pub client_id: String,
    pub client_secret: String,
}

/// Why a pasted value is not accepted. The words are made when the line is drawn (`Display`), so
/// an error that stays on screen follows a language switch. Shared by the cloud drives and
/// YouTube.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldError {
    /// The client ID field is empty (the wording follows the provider).
    IdMissing(Kind),
    IdSpaces,
    /// The client ID is not in the provider's shape.
    IdWrongShape(Kind),
    /// The Google client ID of YouTube does not end with this suffix.
    NotGoogleId(&'static str),
    SecretMissing,
    SecretSpaces,
    /// The provider named `name` asks for no secret.
    NoSecretNeeded(&'static str),
}

impl std::fmt::Display for FieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self {
            FieldError::IdMissing(kind) => match kind {
                Kind::GoogleDrive => tr!("Paste the client ID."),
                Kind::OneDrive => tr!("Paste the application (client) ID."),
                Kind::Dropbox | Kind::Local => tr!("Paste the app key."),
            },
            FieldError::IdSpaces => tr!("The client ID has no spaces in it; copy it again."),
            FieldError::IdWrongShape(kind) => match kind {
                Kind::GoogleDrive => tr!(
                    "That doesn\u{2019}t look like a Google Drive client ID. It ends with .apps.googleusercontent.com."
                ),
                Kind::OneDrive => tr!(
                    "That doesn\u{2019}t look like a OneDrive application (client) ID. It is 32 hexadecimal digits in groups, like 11111111-2222-3333-4444-555555555555; copy \u{201c}Application (client) ID\u{201d} from the app\u{2019}s Overview page."
                ),
                Kind::Dropbox | Kind::Local => tr!(
                    "That doesn\u{2019}t look like a Dropbox app key. It is the App key on the Settings tab of your app in the App Console: letters and digits, no spaces."
                ),
            },
            FieldError::NotGoogleId(suffix) => tr!(
                "That doesn\u{2019}t look like a Google client ID. It ends with {suffix}.",
                suffix = suffix
            ),
            FieldError::SecretMissing => tr!("Paste the client secret."),
            FieldError::SecretSpaces => {
                tr!("The client secret has no spaces in it; copy it again.")
            }
            FieldError::NoSecretNeeded(name) => {
                tr!("{name} needs no secret; leave this empty.", name = name)
            }
        })
    }
}

/// Why a field is not accepted, by field.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetupErrors {
    pub client_id: Option<FieldError>,
    pub client_secret: Option<FieldError>,
}

/// Checks the pasted values without any network: both trimmed, no spaces inside, the ID in the
/// provider's shape, a secret when the provider needs one.
pub fn validate_setup(
    p: &ProviderInfo,
    form: &SetupForm,
) -> Result<ClientCredentials, SetupErrors> {
    let mut errors = SetupErrors::default();
    let id = form.client_id.trim();
    if id.is_empty() {
        errors.client_id = Some(FieldError::IdMissing(p.kind));
    } else if id.chars().any(char::is_whitespace) {
        errors.client_id = Some(FieldError::IdSpaces);
    } else if !(p.id_ok)(id) {
        errors.client_id = Some(FieldError::IdWrongShape(p.kind));
    }
    let secret = form.client_secret.trim();
    if p.needs_secret() && secret.is_empty() {
        errors.client_secret = Some(FieldError::SecretMissing);
    } else if !p.needs_secret() && !secret.is_empty() {
        // Not asked for, so a value here is a mistake that would only be sent to the provider.
        errors.client_secret = Some(FieldError::NoSecretNeeded(p.name));
    } else if secret.chars().any(char::is_whitespace) {
        errors.client_secret = Some(FieldError::SecretSpaces);
    }
    if errors != SetupErrors::default() {
        return Err(errors);
    }
    Ok(ClientCredentials {
        client_id: id.to_owned(),
        client_secret: (!secret.is_empty()).then(|| secret.to_owned()),
    })
}

// ---- the flow ----------------------------------------------------------------------------------

/// Everything the flow talks to. The product wires the real ones with [`Env::system`].
pub struct Env {
    pub secrets: Arc<dyn SecretStore>,
    /// The token endpoint.
    pub auth_http: Arc<dyn HttpClient>,
    pub browser: Arc<dyn BrowserOpener>,
    /// The provider's API (Drive, Graph or Dropbox).
    pub storage_http: Arc<dyn StorageHttp>,
    /// The daemon's token source; the flow clears its cache after connecting.
    pub tokens: Arc<KeychainTokens>,
    pub store: DestinationStore,
    pub timeout: Duration,
}

impl Env {
    /// The OS keychain, the system browser and the real network, writing `destinations.json` in
    /// `store`. Shares the process's token cache with the daemon's uploads.
    pub fn system(store: DestinationStore) -> Env {
        Env {
            secrets: Arc::new(cm_auth::VaultStore::system(
                cm_daemon::storage_cloud::KEYCHAIN_SERVICE,
            )),
            auth_http: Arc::new(UreqClient::new()),
            browser: Arc::new(SystemBrowser),
            storage_http: Arc::new(UreqHttp::new()),
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
    /// Asking which account signed in.
    Account,
    /// Creating the "CaptureMann" folder (not for Dropbox, which creates it with the first file).
    Folder,
    /// Writing the destination.
    Saving,
}

impl Phase {
    pub fn label(self) -> String {
        match self {
            Phase::Browser => tr!("Waiting for you to sign in in the browser\u{2026}"),
            Phase::Account => tr!("Checking the account\u{2026}"),
            Phase::Folder => tr!("Creating the CaptureMann folder\u{2026}"),
            Phase::Saving => tr!("Saving the destination\u{2026}"),
        }
    }
}

/// A new destination, or a new sign-in for one that exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    New,
    Reconnect { destination_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connected {
    pub destination_id: String,
    pub email: String,
    pub folder_name: String,
    pub reconnected: bool,
    /// Something to tell the user about this account (personal or work, link modes); worded
    /// when drawn ([`AccountNote::text`]).
    pub note: Option<AccountNote>,
}

/// What the user is told about the account that just signed in. Kept as a reason so the line
/// follows a language switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountNote {
    /// OneDrive, a personal Microsoft account.
    PersonalMicrosoft,
    /// OneDrive, a work or school account.
    WorkMicrosoft,
    /// Dropbox with "App folder" access.
    DropboxAppFolder,
    /// Dropbox with full access.
    DropboxFull,
}

impl AccountNote {
    pub fn text(self) -> String {
        match self {
            AccountNote::PersonalMicrosoft => tr!(
                "This is a personal Microsoft account: links are for anyone with the link, or none. \u{201c}People in my organization\u{201d} links need a work or school account."
            ),
            AccountNote::WorkMicrosoft => tr!(
                "This is a work or school account. If your administrator forbids public links, CaptureMann makes a link for people in your organization instead."
            ),
            AccountNote::DropboxAppFolder => {
                tr!("Files go into your app\u{2019}s own folder under Apps in Dropbox.")
            }
            AccountNote::DropboxFull => {
                tr!("Files go into a CaptureMann folder at the top of your Dropbox.")
            }
        }
    }
}

/// What the setup screen chose besides the credentials.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// Dropbox: the app has "App folder" access.
    pub app_folder: bool,
}

/// What [`verify`] learned about the account.
struct Verified {
    label: String,
    folder_id: Option<String>,
    note: Option<AccountNote>,
}

/// A fresh account key such as `gdrive-k3x9a2bq`: lower-case letters and digits.
pub fn new_account_key(p: &ProviderInfo) -> Result<String, CmError> {
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
    Ok(format!("{}-{suffix}", p.key))
}

/// Runs the whole connect flow. Nothing is stored until the browser sign-in succeeded. A new
/// connection that fails after that leaves no secret behind; a reconnect keeps the new sign-in.
pub async fn connect(
    env: &Env,
    p: &ProviderInfo,
    creds: ClientCredentials,
    target: &Target,
    phase: &(dyn Fn(Phase) + Send + Sync),
) -> Result<Connected, CmError> {
    connect_with(env, p, creds, target, Options::default(), phase).await
}

/// [`connect`] with the choices the provider's setup screen offers.
pub async fn connect_with(
    env: &Env,
    p: &ProviderInfo,
    creds: ClientCredentials,
    target: &Target,
    opts: Options,
    phase: &(dyn Fn(Phase) + Send + Sync),
) -> Result<Connected, CmError> {
    let opts = Options {
        app_folder: opts.app_folder && p.access_choice,
    };
    let reconnecting = match target {
        Target::New => None,
        Target::Reconnect { destination_id } => {
            let dest = env.store.get(destination_id)?;
            let account = match (&dest.account, dest.kind == p.kind) {
                (Some(a), true) => a.account.clone(),
                _ => {
                    return Err(CmError::invalid_input(tr!(
                        "\u{201c}{name}\u{201d} is not a {provider} destination.",
                        name = dest.name,
                        provider = p.name
                    )))
                }
            };
            Some((dest.id, account))
        }
    };
    let config = (p.config)();
    phase(Phase::Browser);
    let tokens = AuthFlow {
        provider: &config,
        creds: &creds,
        http: env.auth_http.as_ref(),
        browser: env.browser.as_ref(),
    }
    .authorize(env.timeout)
    .await?;

    let account = match &reconnecting {
        Some((_, account)) => account.clone(),
        None => new_account_key(p)?,
    };
    let manager = TokenManager::new(
        config.clone(),
        creds.clone(),
        env.auth_http.clone(),
        env.secrets.clone(),
    );
    let stored = save_credentials(env.secrets.as_ref(), config.id, &account, &creds)
        .and_then(|()| manager.save(&account, &tokens));
    env.tokens.invalidate(&account);
    let result = match stored {
        Ok(()) => {
            finish(
                env,
                p,
                &manager,
                &account,
                reconnecting.as_ref(),
                opts,
                phase,
            )
            .await
        }
        Err(e) => Err(e),
    };
    if result.is_err() && reconnecting.is_none() {
        // A half-connected account would only confuse the next try.
        let _ = manager.forget(&account);
        let _ = forget_credentials(env.secrets.as_ref(), config.id, &account);
        env.tokens.invalidate(&account);
    }
    result
}

async fn finish(
    env: &Env,
    p: &ProviderInfo,
    manager: &TokenManager,
    account: &str,
    reconnecting: Option<&(String, String)>,
    opts: Options,
    phase: &(dyn Fn(Phase) + Send + Sync),
) -> Result<Connected, CmError> {
    let Verified {
        label: email,
        folder_id,
        note,
    } = verify(env, p, account, opts, phase).await?;
    // Remember who it is next to the tokens ("shown in Settings").
    if let Some(mut t) = manager.load(account)? {
        t.account_label = Some(email.clone());
        manager.save(account, &t)?;
    }
    phase(Phase::Saving);
    let mut list = env.store.list()?;
    let cloud = CloudAccount {
        account: account.to_owned(),
        label: email.clone(),
        folder_id,
        folder_name: FOLDER_NAME.to_owned(),
        link_mode: LinkMode::Anyone,
        app_folder: opts.app_folder,
    };
    let destination_id = match reconnecting {
        Some((id, _)) => {
            let d = list
                .iter_mut()
                .find(|d| &d.id == id)
                .ok_or_else(|| CmError::not_found(tr!("That destination no longer exists.")))?;
            let link_mode = d.account.as_ref().map(|a| a.link_mode).unwrap_or_default();
            d.account = Some(CloudAccount { link_mode, ..cloud });
            id.clone()
        }
        None => {
            let name = rules::unique_name(p.name, &list);
            let id = rules::unique_id(&name, &list);
            let default = list.is_empty();
            list.push(Destination {
                id: id.clone(),
                name,
                kind: p.kind,
                path: Default::default(),
                account: Some(cloud),
                folder_pattern: DEFAULT_FOLDER_PATTERN.to_owned(),
                what: What::Export,
                auto_upload: AutoUpload::Off,
                default,
            });
            id
        }
    };
    env.store.save(&list)?;
    Ok(Connected {
        destination_id,
        email,
        folder_name: FOLDER_NAME.to_owned(),
        reconnected: reconnecting.is_some(),
        note,
    })
}

/// The provider-specific half: which account signed in, and the folder id where there is one.
async fn verify(
    env: &Env,
    p: &ProviderInfo,
    account: &str,
    opts: Options,
    phase: &(dyn Fn(Phase) + Send + Sync),
) -> Result<Verified, CmError> {
    match p.kind {
        Kind::GoogleDrive => {
            let drive = drive_provider(env, account, None);
            phase(Phase::Account);
            let label = drive.account_email().await?;
            phase(Phase::Folder);
            let folder_id = drive.ensure_root_folder().await?;
            Ok(Verified {
                label,
                folder_id: Some(folder_id),
                note: None,
            })
        }
        Kind::OneDrive => {
            let drive = onedrive_provider(env, account, None);
            phase(Phase::Account);
            let who = drive.account().await?;
            phase(Phase::Folder);
            let folder_id = drive.ensure_root_folder().await?;
            let note = match who.work {
                Some(false) => Some(AccountNote::PersonalMicrosoft),
                Some(true) => Some(AccountNote::WorkMicrosoft),
                None => None,
            };
            Ok(Verified {
                label: who.label,
                folder_id: Some(folder_id),
                note,
            })
        }
        Kind::Dropbox => {
            let dbx = dropbox_provider(env, account, None, opts.app_folder);
            phase(Phase::Account);
            let label = dbx.account_email().await?;
            Ok(Verified {
                label,
                // Dropbox creates the folders with the first file.
                folder_id: None,
                note: Some(if opts.app_folder {
                    AccountNote::DropboxAppFolder
                } else {
                    AccountNote::DropboxFull
                }),
            })
        }
        Kind::Local => Err(CmError::internal("a local folder is not a cloud provider")),
    }
}

fn drive_provider(env: &Env, account: &str, known: Option<&CloudAccount>) -> GoogleDriveProvider {
    GoogleDriveProvider::new(
        env.storage_http.clone(),
        env.tokens.source(account),
        DriveConfig {
            root_folder_name: known
                .map(|a| a.folder_name.clone())
                .unwrap_or_else(|| FOLDER_NAME.to_owned()),
            root_folder_id: known.and_then(|a| a.folder_id.clone()),
            link_mode: known.map(|a| a.link_mode).unwrap_or_default(),
            ..DriveConfig::default()
        },
    )
}

fn onedrive_provider(env: &Env, account: &str, known: Option<&CloudAccount>) -> OneDriveProvider {
    OneDriveProvider::new(
        env.storage_http.clone(),
        env.tokens.source(account),
        OneDriveConfig {
            root_folder_name: known
                .map(|a| a.folder_name.clone())
                .unwrap_or_else(|| FOLDER_NAME.to_owned()),
            root_folder_id: known.and_then(|a| a.folder_id.clone()),
            link_mode: known.map(|a| a.link_mode).unwrap_or_default(),
            ..OneDriveConfig::default()
        },
    )
}

fn dropbox_provider(
    env: &Env,
    account: &str,
    known: Option<&CloudAccount>,
    app_folder: bool,
) -> DropboxProvider {
    DropboxProvider::new(
        env.storage_http.clone(),
        env.tokens.source(account),
        DropboxConfig {
            root_folder_name: known
                .map(|a| a.folder_name.clone())
                .unwrap_or_else(|| FOLDER_NAME.to_owned()),
            app_folder: known.map(|a| a.app_folder).unwrap_or(app_folder),
            link_mode: known.map(|a| a.link_mode).unwrap_or_default(),
            ..DropboxConfig::default()
        },
    )
}

/// "Test connection": uploads and deletes a 1 KB file with the destination's own account.
pub async fn test_connection(env: &Env, dest: &Destination) -> Result<(), CmError> {
    let account = dest.account.as_ref().ok_or_else(|| {
        CmError::invalid_input(tr!("This destination has no account; connect it first."))
    })?;
    match dest.kind {
        Kind::GoogleDrive => {
            drive_provider(env, &account.account, Some(account))
                .test_connection()
                .await
        }
        Kind::OneDrive => {
            onedrive_provider(env, &account.account, Some(account))
                .test_connection()
                .await
        }
        Kind::Dropbox => {
            dropbox_provider(env, &account.account, Some(account), account.app_folder)
                .test_connection()
                .await
        }
        Kind::Local => Err(CmError::invalid_input(tr!(
            "A local folder has no connection to test."
        ))),
    }
}

// ---- state of an account -----------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountState {
    Connected,
    /// The sign-in is gone or was refused: the row shows "Reconnect" with this reason.
    Reconnect(ReconnectWhy),
}

/// Why a sign-in has to be redone. Kept as a reason and worded when drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconnectWhy {
    /// The destination has no account.
    NoAccount,
    /// No tokens or credentials in the keychain.
    NotSignedIn,
    /// Uploads wait for a new sign-in (`auth_expired`).
    UploadsWaiting,
    /// The provider refused the sign-in.
    Refused,
    /// The keychain or another part said this (the OS text).
    Os(String),
}

impl ReconnectWhy {
    pub fn text(&self) -> String {
        match self {
            ReconnectWhy::NoAccount => tr!("No account is set for this destination."),
            ReconnectWhy::NotSignedIn => tr!("Not signed in on this Mac."),
            ReconnectWhy::UploadsWaiting => tr!("The sign-in expired; uploads are waiting."),
            ReconnectWhy::Refused => tr!("The sign-in was refused."),
            ReconnectWhy::Os(e) => e.clone(),
        }
    }
}

/// Whether a cloud destination can upload now, without the network: its tokens and credentials
/// are in the secret store and no upload job is waiting for it as `auth_expired`
/// (`paused`: ids from [`auth_paused_destinations`]). Reads the keychain, so call it off the UI
/// thread.
pub fn account_state(
    secrets: &dyn SecretStore,
    dest: &Destination,
    paused: &HashSet<String>,
) -> AccountState {
    let Some(provider) = provider_of(dest) else {
        return AccountState::Connected;
    };
    let Some(account) = &dest.account else {
        return AccountState::Reconnect(ReconnectWhy::NoAccount);
    };
    let config = (provider.config)();
    let token_key = format!("oauth.{}.{}", config.id, account.account);
    match (
        secrets.get(&token_key),
        load_credentials(secrets, config.id, &account.account),
    ) {
        (Ok(Some(_)), Ok(Some(_))) => {}
        (Ok(None), _) | (_, Ok(None)) => return AccountState::Reconnect(ReconnectWhy::NotSignedIn),
        (Err(e), _) | (_, Err(e)) => {
            return AccountState::Reconnect(ReconnectWhy::Os(e.to_string()))
        }
    }
    if paused.contains(&dest.id) {
        return AccountState::Reconnect(ReconnectWhy::UploadsWaiting);
    }
    AccountState::Connected
}

/// The destinations that have an upload waiting as `auth_expired`, from a `list_jobs` result.
pub fn auth_paused_destinations(jobs: &Value) -> HashSet<String> {
    jobs["jobs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|j| {
            j["type"] == "upload" && j["state"] == "paused" && j["pause_reason"] == "auth_expired"
        })
        .filter_map(|j| j["inputs"]["destination_id"].as_str())
        .map(str::to_owned)
        .collect()
}

/// Like [`disconnect`], but first asks Google to revoke the sign-in (best effort, STO-2): only for
/// a Google Drive account that no other destination uses, and never for the other providers,
/// which have no revoke call here. A revoke that fails (offline, refused) does not stop the local
/// sign-out; the outcome says what happened and [`sign_out_message`] puts it in words. An unknown
/// destination is `not_found` and nothing is sent.
pub async fn disconnect_and_revoke(
    env: &Env,
    destination_id: &str,
) -> Result<RevokeOutcome, CmError> {
    let list = env.store.list()?;
    let dest = list
        .iter()
        .find(|d| d.id == destination_id)
        .ok_or_else(|| CmError::not_found(tr!("That destination no longer exists.")))?;
    let outcome = match (dest.account.as_ref(), provider_of(dest)) {
        (Some(account), Some(provider)) => {
            let shared = list.iter().any(|d| {
                d.id != dest.id
                    && d.account
                        .as_ref()
                        .is_some_and(|a| a.account == account.account)
            });
            if shared {
                RevokeOutcome::NotAttempted
            } else {
                revoke_account(
                    env.auth_http.as_ref(),
                    env.secrets.as_ref(),
                    &(provider.config)(),
                    &account.account,
                )
                .await
            }
        }
        _ => RevokeOutcome::NotAttempted,
    };
    disconnect(env, destination_id)?;
    Ok(outcome)
}

/// What the user reads after "Disconnect" for a cloud destination called `name`.
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

/// Removes the destination and, when no other destination uses the account, its tokens and
/// credentials. Nothing is revoked at the provider (see [`disconnect_and_revoke`]) and no remote
/// file is touched.
pub fn disconnect(env: &Env, destination_id: &str) -> Result<(), CmError> {
    let mut list = env.store.list()?;
    let gone = rules::remove(&mut list, destination_id)
        .ok_or_else(|| CmError::not_found(tr!("That destination no longer exists.")))?;
    env.store.save(&list)?;
    let (Some(account), Some(provider)) = (gone.account.as_ref(), provider_of(&gone)) else {
        return Ok(());
    };
    let shared = list.iter().any(|d| {
        d.account
            .as_ref()
            .is_some_and(|a| a.account == account.account)
    });
    if !shared {
        let config = (provider.config)();
        env.secrets
            .delete(&format!("oauth.{}.{}", config.id, account.account))?;
        forget_credentials(env.secrets.as_ref(), config.id, &account.account)?;
        env.tokens.invalidate(&account.account);
    }
    Ok(())
}

#[cfg(test)]
mod providers_tests;
#[cfg(test)]
mod tests;
