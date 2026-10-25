//! The connect flow against fakes: a fake browser that plays the user, a scripted sign-in server,
//! the in-process fake Drive and an in-memory secret store. No network, no keychain, no browser.

#![allow(clippy::unwrap_used)]

use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use cm_auth::{HttpError, HttpResponse, MemoryStore};
use cm_core::ErrorCode;
use cm_storage::fake_drive::{Call, FakeDrive, Fault};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::*;

const GOOD_ID: &str = "1234-abcd.apps.googleusercontent.com";
const SECRET: &str = "GOCSPX-client-secret-value";
const EXCHANGE: &str = r#"{"access_token":"tok-1","refresh_token":"refresh-1","expires_in":3600}"#;

/// Plays the user: reads `redirect_uri` and `state` from the URL, then calls the loopback back.
pub(super) struct FakeBrowser {
    pub(super) opened: Mutex<Vec<String>>,
    /// `false`: nobody answers (the user closes the tab).
    pub(super) answer: bool,
}

impl BrowserOpener for FakeBrowser {
    fn open(&self, url: &str) -> Result<(), CmError> {
        self.opened.lock().unwrap().push(url.to_owned());
        if !self.answer {
            return Ok(());
        }
        let query = url.split_once('?').unwrap().1;
        let get = |k: &str| {
            query
                .split('&')
                .find_map(|kv| kv.strip_prefix(&format!("{k}=")))
                .unwrap()
                .to_owned()
        };
        let redirect = get("redirect_uri").replace("%3A", ":").replace("%2F", "/");
        let port: u16 = redirect
            .split(':')
            .nth(2)
            .unwrap()
            .split('/')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let state = get("state");
        tokio::spawn(async move {
            let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            let req =
                format!("GET /callback?code=the-code&state={state} HTTP/1.1\r\nHost: x\r\n\r\n");
            s.write_all(req.as_bytes()).await.unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out).await;
        });
        Ok(())
    }
}

/// A request to the token endpoint: the URL and the form fields.
pub(super) type FormCall = (String, Vec<(String, String)>);

/// The token endpoint: answers in order.
pub(super) struct SignIn {
    pub(super) answers: Mutex<VecDeque<(u16, String)>>,
    /// Every request: the URL and the form fields.
    pub(super) calls: Mutex<Vec<FormCall>>,
}

#[async_trait]
impl HttpClient for SignIn {
    async fn post_form(
        &self,
        url: &str,
        form: &[(String, String)],
    ) -> Result<HttpResponse, HttpError> {
        self.calls
            .lock()
            .unwrap()
            .push((url.to_owned(), form.to_vec()));
        let (status, body) = self
            .answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or((500, String::new()));
        Ok(HttpResponse { status, body })
    }
}

struct Rig {
    env: Env,
    signin: Arc<SignIn>,
    drive: Arc<FakeDrive>,
    secrets: Arc<MemoryStore>,
    browser: Arc<FakeBrowser>,
    config: tempfile::TempDir,
}

fn rig_with(answers: &[(u16, &str)], user_answers: bool) -> Rig {
    let config = tempfile::tempdir().unwrap();
    let secrets = Arc::new(MemoryStore::new());
    let drive = Arc::new(FakeDrive::new());
    drive.set_account_email("teacher@school.example");
    let signin = Arc::new(SignIn {
        answers: Mutex::new(answers.iter().map(|(s, b)| (*s, (*b).to_owned())).collect()),
        calls: Mutex::new(Vec::new()),
    });
    let browser = Arc::new(FakeBrowser {
        opened: Mutex::new(Vec::new()),
        answer: user_answers,
    });
    let env = Env {
        secrets: secrets.clone(),
        auth_http: signin.clone(),
        browser: browser.clone(),
        storage_http: drive.clone(),
        tokens: KeychainTokens::new(secrets.clone(), signin.clone()),
        store: DestinationStore::new(config.path()),
        timeout: Duration::from_secs(5),
    };
    Rig {
        signin,
        env,
        drive,
        secrets,
        browser,
        config,
    }
}

fn rig() -> Rig {
    rig_with(&[(200, EXCHANGE)], true)
}

fn creds() -> ClientCredentials {
    ClientCredentials {
        client_id: GOOD_ID.into(),
        client_secret: Some(SECRET.into()),
    }
}

async fn connect_new(r: &Rig) -> Result<Connected, CmError> {
    connect(&r.env, &GOOGLE_DRIVE, creds(), &Target::New, &|_| {}).await
}

// ---- the setup screen ---------------------------------------------------------------------------

#[test]
fn setup_values_are_checked_before_anything_happens() {
    let ok = validate_setup(
        &GOOGLE_DRIVE,
        &SetupForm {
            client_id: format!("  {GOOD_ID}\n"),
            client_secret: format!(" {SECRET} "),
        },
    )
    .unwrap();
    assert_eq!(ok.client_id, GOOD_ID);
    assert_eq!(ok.client_secret.as_deref(), Some(SECRET));

    let e = validate_setup(&GOOGLE_DRIVE, &SetupForm::default()).unwrap_err();
    assert!(e
        .client_id
        .unwrap()
        .to_string()
        .contains("Paste the client ID"));
    assert!(e
        .client_secret
        .unwrap()
        .to_string()
        .contains("client secret"));

    let e = validate_setup(
        &GOOGLE_DRIVE,
        &SetupForm {
            client_id: "my-project-id".into(),
            client_secret: SECRET.into(),
        },
    )
    .unwrap_err();
    assert!(e
        .client_id
        .unwrap()
        .to_string()
        .contains("apps.googleusercontent.com"));
    assert!(e.client_secret.is_none());

    let e = validate_setup(
        &GOOGLE_DRIVE,
        &SetupForm {
            client_id: format!("{GOOD_ID} extra"),
            client_secret: "a b".into(),
        },
    )
    .unwrap_err();
    assert!(e.client_id.unwrap().to_string().contains("no spaces"));
    assert!(e.client_secret.unwrap().to_string().contains("no spaces"));
}

#[test]
fn only_providers_with_a_flow_have_a_connect_button() {
    assert!(provider_named("Google Drive").is_some());
    assert!(provider_named("OneDrive").is_some());
    assert!(provider_named("Dropbox").is_some());
    assert!(provider_named("Box").is_none());
    // v0.1 offers Google Drive only; the others are built but say "Coming soon" (Oct 23, 2026).
    assert!(is_offered("Google Drive"));
    assert!(!is_offered("OneDrive") && !is_offered("Dropbox"));
    assert!(GOOGLE_DRIVE.steps().len() >= 4);
    assert!(GOOGLE_DRIVE.note().contains("drive.file"));
}

#[test]
fn account_keys_are_unique_and_safe_in_a_key_name() {
    let a = new_account_key(&GOOGLE_DRIVE).unwrap();
    let b = new_account_key(&GOOGLE_DRIVE).unwrap();
    assert_ne!(a, b);
    assert!(a.starts_with("gdrive-") && a.len() == "gdrive-".len() + 8);
    assert!(a
        .chars()
        .all(|c| c == '-' || c.is_ascii_lowercase() || c.is_ascii_digit()));
}

// ---- connecting ---------------------------------------------------------------------------------

/// STO-2.AC1: the account email shows and a "CaptureMann" folder exists in the drive.
#[tokio::test]
async fn connecting_stores_secrets_in_the_keychain_and_only_key_names_in_the_config() {
    let r = rig();
    let phases = Mutex::new(Vec::new());
    let done = connect(&r.env, &GOOGLE_DRIVE, creds(), &Target::New, &|p| {
        phases.lock().unwrap().push(p)
    })
    .await
    .unwrap();
    assert_eq!(done.email, "teacher@school.example");
    assert_eq!(done.folder_name, "CaptureMann");
    assert!(!done.reconnected);
    assert_eq!(
        *phases.lock().unwrap(),
        [Phase::Browser, Phase::Account, Phase::Folder, Phase::Saving]
    );

    // Drive: the CaptureMann folder exists.
    let folders = r.drive.folders();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].name, "CaptureMann");

    // The destination.
    let list = r.env.store.list().unwrap();
    assert_eq!(list.len(), 1);
    let d = &list[0];
    assert_eq!(d.id, done.destination_id);
    assert_eq!(d.kind, Kind::GoogleDrive);
    assert_eq!(d.name, "Google Drive");
    assert!(d.default, "the first destination is the default");
    assert_eq!(d.what, What::Export);
    let acc = d.account.as_ref().unwrap();
    assert_eq!(acc.label, "teacher@school.example");
    assert_eq!(acc.folder_id.as_deref(), Some(folders[0].id.as_str()));
    assert_eq!(acc.folder_name, "CaptureMann");
    assert_eq!(acc.link_mode, LinkMode::Anyone);

    // The keychain holds the tokens and the credentials under keys the config names.
    let token_key = format!("oauth.google.{}", acc.account);
    let tokens = r.secrets.get(&token_key).unwrap().unwrap();
    assert!(tokens.contains("tok-1") && tokens.contains("refresh-1"));
    assert!(tokens.contains("teacher@school.example"), "label kept");
    let client = r
        .secrets
        .get(&format!("{token_key}.client"))
        .unwrap()
        .unwrap();
    assert!(client.contains(GOOD_ID) && client.contains(SECRET));

    // The config file holds no secret: not the tokens, not the client ID or its secret.
    let file = std::fs::read_to_string(r.config.path().join("destinations.json")).unwrap();
    for secret in ["tok-1", "refresh-1", SECRET, GOOD_ID] {
        assert!(!file.contains(secret), "{secret} leaked into {file}");
    }
    assert!(file.contains(&acc.account), "only the key name is stored");

    // The browser URL carried PKCE and the client ID, never the secret.
    let url = r.browser.opened.lock().unwrap()[0].clone();
    assert!(url.contains("code_challenge_method=S256") && url.contains("drive.file"));
    assert!(!url.contains(SECRET));
}

#[tokio::test]
async fn the_destination_works_for_uploads_through_the_daemons_token_source() {
    // What was stored is enough for the daemon: the same keychain, a Drive call with the token.
    let r = rig();
    let done = connect_new(&r).await.unwrap();
    let dest = r.env.store.get(&done.destination_id).unwrap();
    test_connection(&r.env, &dest).await.unwrap();
    assert!(r.drive.files().is_empty(), "the test file is deleted again");
    assert_eq!(r.drive.count(Call::Delete), 1);
}

#[tokio::test]
async fn a_second_account_is_a_second_destination_and_not_the_default() {
    let r = rig_with(&[(200, EXCHANGE), (200, EXCHANGE)], true);
    let a = connect_new(&r).await.unwrap();
    r.drive.set_account_email("other@school.example");
    let b = connect_new(&r).await.unwrap();
    let list = r.env.store.list().unwrap();
    assert_eq!(list.len(), 2);
    assert_ne!(a.destination_id, b.destination_id);
    assert_eq!(list[1].name, "Google Drive (2)");
    assert_eq!(list.iter().filter(|d| d.default).count(), 1);
    assert!(list[0].default);
    assert_ne!(
        list[0].account.as_ref().unwrap().account,
        list[1].account.as_ref().unwrap().account
    );
}

#[tokio::test]
async fn a_user_who_never_answers_leaves_nothing_behind() {
    let mut r = rig_with(&[(200, EXCHANGE)], false);
    r.env.timeout = Duration::from_millis(200);
    let err = connect_new(&r).await.unwrap_err();
    assert!(
        matches!(
            err.code(),
            ErrorCode::UserDeclined | ErrorCode::ConfirmationTimeout | ErrorCode::Network
        ),
        "{err}"
    );
    assert!(r.env.store.list().unwrap().is_empty());
    assert!(format!("{:?}", r.secrets).contains("keys: []"));
}

#[tokio::test]
async fn the_provider_refusing_the_code_leaves_nothing_behind() {
    let r = rig_with(
        &[(
            400,
            r#"{"error":"invalid_client","error_description":"bad"}"#,
        )],
        true,
    );
    let err = connect_new(&r).await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::PlatformRejected);
    assert!(r.env.store.list().unwrap().is_empty());
}

#[tokio::test]
async fn a_failure_after_the_sign_in_removes_the_new_secrets_and_adds_no_destination() {
    let r = rig();
    // Drive refuses the account lookup (the Drive API is not enabled in the project).
    r.drive.inject(
        Call::About,
        0,
        Fault::Status {
            status: 403,
            retry_after: None,
            reason: "accessNotConfigured",
            message: "Google Drive API has not been used in project 123 before.",
        },
        1,
    );
    let err = connect_new(&r).await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::PlatformRejected);
    assert!(err.to_string().contains("Google Drive API"), "{err}");
    assert!(r.env.store.list().unwrap().is_empty());
    assert!(
        format!("{:?}", r.secrets).contains("keys: []"),
        "no half-connected account stays in the keychain"
    );
}

#[tokio::test]
async fn reconnecting_replaces_the_sign_in_of_the_same_destination() {
    let r = rig_with(
        &[
            (200, EXCHANGE),
            (200, EXCHANGE.replace("tok-1", "tok-2").leak()),
        ],
        true,
    );
    let first = connect_new(&r).await.unwrap();
    // The user tuned the destination; the sign-in expired.
    let mut list = r.env.store.list().unwrap();
    list[0].name = "School Drive".into();
    list[0].what = What::Both;
    list[0].account.as_mut().unwrap().link_mode = LinkMode::None;
    r.env.store.save(&list).unwrap();
    let account = list[0].account.as_ref().unwrap().account.clone();
    r.drive.set_valid_token("tok-2");

    let again = connect(
        &r.env,
        &GOOGLE_DRIVE,
        creds(),
        &Target::Reconnect {
            destination_id: first.destination_id.clone(),
        },
        &|_| {},
    )
    .await
    .unwrap();
    assert!(again.reconnected);
    assert_eq!(again.destination_id, first.destination_id);
    let list = r.env.store.list().unwrap();
    assert_eq!(list.len(), 1, "no second destination");
    let d = &list[0];
    assert_eq!(d.name, "School Drive");
    assert_eq!(d.what, What::Both);
    let acc = d.account.as_ref().unwrap();
    assert_eq!(acc.account, account, "same account key");
    assert_eq!(acc.link_mode, LinkMode::None, "the user's choice is kept");
    let tokens = r
        .secrets
        .get(&format!("oauth.google.{account}"))
        .unwrap()
        .unwrap();
    assert!(tokens.contains("tok-2"));
    assert_eq!(
        r.drive.folders().len(),
        1,
        "the folder is found, not duplicated"
    );
}

#[tokio::test]
async fn reconnect_refuses_a_destination_that_is_not_this_provider() {
    let r = rig();
    let mut local = Destination {
        id: "dest_nas".into(),
        name: "NAS".into(),
        kind: Kind::Local,
        path: "/nowhere".into(),
        account: None,
        folder_pattern: DEFAULT_FOLDER_PATTERN.into(),
        what: What::Original,
        auto_upload: AutoUpload::Off,
        default: true,
    };
    local.path = r.config.path().to_path_buf();
    r.env.store.save(&[local]).unwrap();
    let err = connect(
        &r.env,
        &GOOGLE_DRIVE,
        creds(),
        &Target::Reconnect {
            destination_id: "dest_nas".into(),
        },
        &|_| {},
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(
        r.browser.opened.lock().unwrap().is_empty(),
        "no browser for a request that cannot work"
    );
}

// ---- state, test connection, disconnect ---------------------------------------------------------

#[tokio::test]
async fn an_account_needs_reconnect_when_its_secrets_are_gone_or_uploads_wait() {
    let r = rig();
    let done = connect_new(&r).await.unwrap();
    let dest = r.env.store.get(&done.destination_id).unwrap();
    let none = HashSet::new();
    assert_eq!(
        account_state(r.secrets.as_ref(), &dest, &none),
        AccountState::Connected
    );

    let waiting: HashSet<String> = [dest.id.clone()].into();
    assert!(matches!(
        account_state(r.secrets.as_ref(), &dest, &waiting),
        AccountState::Reconnect(m) if m.text().contains("expired")
    ));

    let account = &dest.account.as_ref().unwrap().account;
    r.secrets
        .delete(&format!("oauth.google.{account}.client"))
        .unwrap();
    assert!(matches!(
        account_state(r.secrets.as_ref(), &dest, &none),
        AccountState::Reconnect(m) if m.text().contains("Not signed in")
    ));
    r.secrets
        .delete(&format!("oauth.google.{account}"))
        .unwrap();
    assert!(matches!(
        account_state(r.secrets.as_ref(), &dest, &none),
        AccountState::Reconnect(_)
    ));
}

#[test]
fn waiting_uploads_are_found_by_destination_in_a_job_list() {
    let jobs = json!({ "jobs": [
        { "type": "upload", "state": "paused", "pause_reason": "auth_expired",
          "inputs": { "destination_id": "drive" } },
        { "type": "upload", "state": "paused", "pause_reason": "user",
          "inputs": { "destination_id": "other" } },
        { "type": "export", "state": "paused", "pause_reason": "auth_expired", "inputs": {} },
        { "type": "upload", "state": "done", "pause_reason": null,
          "inputs": { "destination_id": "third" } },
    ]});
    let found = auth_paused_destinations(&jobs);
    assert_eq!(found, HashSet::from(["drive".to_owned()]));
    assert!(auth_paused_destinations(&json!({})).is_empty());
}

#[tokio::test]
async fn test_connection_reports_a_revoked_sign_in_as_auth_expired() {
    let r = rig_with(
        &[(200, EXCHANGE), (400, r#"{"error":"invalid_grant"}"#)],
        true,
    );
    let done = connect_new(&r).await.unwrap();
    let dest = r.env.store.get(&done.destination_id).unwrap();
    // Drive stops accepting the token and the refresh is refused.
    r.drive.set_valid_token("other");
    let err = test_connection(&r.env, &dest).await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::AuthExpired, "{err}");
    // A local folder has nothing to test.
    let mut local = dest.clone();
    local.kind = Kind::Local;
    local.account = None;
    assert_eq!(
        test_connection(&r.env, &local).await.unwrap_err().code(),
        ErrorCode::InvalidInput
    );
}

#[tokio::test]
async fn disconnecting_forgets_the_destination_and_the_secrets_but_nothing_remote() {
    let r = rig();
    let done = connect_new(&r).await.unwrap();
    let account = r
        .env
        .store
        .get(&done.destination_id)
        .unwrap()
        .account
        .unwrap()
        .account;
    let folders_before = r.drive.folders().len();
    disconnect(&r.env, &done.destination_id).unwrap();
    assert!(r.env.store.list().unwrap().is_empty());
    assert!(r
        .secrets
        .get(&format!("oauth.google.{account}"))
        .unwrap()
        .is_none());
    assert!(r
        .secrets
        .get(&format!("oauth.google.{account}.client"))
        .unwrap()
        .is_none());
    assert_eq!(
        r.drive.folders().len(),
        folders_before,
        "the Drive is untouched"
    );
    assert_eq!(
        disconnect(&r.env, &done.destination_id).unwrap_err().code(),
        ErrorCode::NotFound
    );
}

/// STO-2: Disconnecting a Google Drive destination asks Google to revoke the refresh token first.
#[tokio::test]
async fn disconnecting_drive_revokes_the_refresh_token_at_google() {
    let r = rig_with(&[(200, EXCHANGE), (200, "{}")], true);
    let done = connect_new(&r).await.unwrap();
    let outcome = disconnect_and_revoke(&r.env, &done.destination_id)
        .await
        .unwrap();
    assert_eq!(outcome, RevokeOutcome::Revoked);
    {
        let calls = r.signin.calls.lock().unwrap();
        let (url, form) = calls.last().unwrap();
        assert_eq!(url, "https://oauth2.googleapis.com/revoke");
        assert_eq!(form.len(), 1);
        assert_eq!(form[0].0, "token");
        assert!(!form[0].1.is_empty());
    }
    assert!(r.env.store.list().unwrap().is_empty());
    assert!(format!("{:?}", r.secrets).contains("keys: []"));
    assert!(sign_out_message("My Drive", &outcome).contains("Google was told"));
}

#[tokio::test]
async fn a_failed_drive_revoke_still_disconnects_and_says_where_to_finish() {
    let r = rig_with(&[(200, EXCHANGE)], true);
    let done = connect_new(&r).await.unwrap();
    let outcome = disconnect_and_revoke(&r.env, &done.destination_id)
        .await
        .unwrap();
    assert!(outcome.needs_manual_removal(), "{outcome:?}");
    assert!(r.env.store.list().unwrap().is_empty());
    assert!(format!("{:?}", r.secrets).contains("keys: []"));
    let msg = sign_out_message("My Drive", &outcome);
    assert!(msg.contains("signed out here"), "{msg}");
    assert!(msg.contains("myaccount.google.com/permissions"), "{msg}");
}

#[tokio::test]
async fn nothing_is_revoked_while_another_destination_uses_the_account() {
    let r = rig();
    let done = connect_new(&r).await.unwrap();
    let mut list = r.env.store.list().unwrap();
    let mut twin = list[0].clone();
    twin.id = "dest_twin".into();
    twin.default = false;
    list.push(twin);
    r.env.store.save(&list).unwrap();
    let before = r.signin.calls.lock().unwrap().len();
    let outcome = disconnect_and_revoke(&r.env, &done.destination_id)
        .await
        .unwrap();
    assert_eq!(outcome, RevokeOutcome::NotAttempted);
    assert_eq!(r.signin.calls.lock().unwrap().len(), before);
    let err = disconnect_and_revoke(&r.env, "dest_missing")
        .await
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::NotFound);
}

#[tokio::test]
async fn an_account_two_destinations_share_keeps_its_secrets_until_the_last_goes() {
    let r = rig();
    let done = connect_new(&r).await.unwrap();
    let mut list = r.env.store.list().unwrap();
    let mut twin = list[0].clone();
    twin.id = "dest_twin".into();
    twin.name = "Twin".into();
    twin.default = false;
    let account = twin.account.as_ref().unwrap().account.clone();
    list.push(twin);
    r.env.store.save(&list).unwrap();
    disconnect(&r.env, &done.destination_id).unwrap();
    assert!(r
        .secrets
        .get(&format!("oauth.google.{account}"))
        .unwrap()
        .is_some());
    disconnect(&r.env, "dest_twin").unwrap();
    assert!(r
        .secrets
        .get(&format!("oauth.google.{account}"))
        .unwrap()
        .is_none());
}

/// I18N P2: the setup texts are built when asked for, in the language of the moment, and keep
/// their placeholders in every language.
#[test]
fn setup_texts_follow_the_language() {
    use cm_i18n::{with_language, Lang};
    let en = GOOGLE_DRIVE.steps();
    let pseudo = with_language(Lang::EnXa, || GOOGLE_DRIVE.steps());
    assert_eq!(en.len(), pseudo.len());
    assert_ne!(en[0], pseudo[0]);
    assert_eq!(
        with_language(Lang::En, || GOOGLE_DRIVE.id_label()),
        "Client ID"
    );
    assert_eq!(DROPBOX.id_label(), "App key");
    assert_eq!(ONEDRIVE.secret_label(), "");
    // The account name survives a translation.
    for lang in [Lang::En, Lang::Vi, Lang::EnXa] {
        with_language(lang, || {
            let m = sign_out_message("Work", &RevokeOutcome::Revoked);
            assert!(m.contains("Work"), "{lang:?}: {m}");
            assert!(!m.contains("{name}"), "{lang:?}: {m}");
        });
    }
    assert_eq!(
        Phase::Folder.label(),
        "Creating the CaptureMann folder\u{2026}"
    );
    assert_ne!(
        with_language(Lang::EnXa, || Phase::Folder.label()),
        Phase::Folder.label()
    );
}
