//! The OneDrive and Dropbox connect flows against the in-process fake Graph and Dropbox servers,
//! a fake browser, a scripted sign-in server and an in-memory secret store (STO-2). "Tested with
//! fakes": no network, no keychain, no browser, no Microsoft or Dropbox account.

#![allow(clippy::unwrap_used)]

use std::sync::Mutex;

use cm_auth::MemoryStore;
use cm_core::ErrorCode;
use cm_storage::fake_drive::Fault;
use cm_storage::fake_dropbox::{self, FakeDropbox};
use cm_storage::fake_onedrive::{self, FakeOneDrive};

use super::tests::{FakeBrowser, SignIn};
use super::*;

const MS_ID: &str = "11111111-2222-3333-4444-abcdefabcdef";
const DBX_KEY: &str = "abcd1234efgh567";
const EXCHANGE: &str = r#"{"access_token":"tok-1","refresh_token":"refresh-1","expires_in":3600}"#;

struct Rig {
    env: Env,
    secrets: Arc<MemoryStore>,
    browser: Arc<FakeBrowser>,
    config: tempfile::TempDir,
}

fn rig_for(http: Arc<dyn StorageHttp>, answers: &[(u16, &str)]) -> Rig {
    let config = tempfile::tempdir().unwrap();
    let secrets = Arc::new(MemoryStore::new());
    let signin = Arc::new(SignIn {
        answers: Mutex::new(answers.iter().map(|(s, b)| (*s, (*b).to_owned())).collect()),
        calls: Mutex::new(Vec::new()),
    });
    let browser = Arc::new(FakeBrowser {
        opened: Mutex::new(Vec::new()),
        answer: true,
    });
    let env = Env {
        secrets: secrets.clone(),
        auth_http: signin.clone(),
        browser: browser.clone(),
        storage_http: http,
        tokens: KeychainTokens::new(secrets.clone(), signin),
        store: DestinationStore::new(config.path()),
        timeout: Duration::from_secs(5),
    };
    Rig {
        env,
        secrets,
        browser,
        config,
    }
}

fn onedrive_rig() -> (Rig, Arc<FakeOneDrive>) {
    let graph = Arc::new(FakeOneDrive::new());
    (rig_for(graph.clone(), &[(200, EXCHANGE)]), graph)
}

fn dropbox_rig() -> (Rig, Arc<FakeDropbox>) {
    let dbx = Arc::new(FakeDropbox::new());
    dbx.set_account_email("teacher@school.example");
    (rig_for(dbx.clone(), &[(200, EXCHANGE)]), dbx)
}

fn ms_creds() -> ClientCredentials {
    ClientCredentials {
        client_id: MS_ID.into(),
        client_secret: None,
    }
}

fn dbx_creds() -> ClientCredentials {
    ClientCredentials {
        client_id: DBX_KEY.into(),
        client_secret: None,
    }
}

fn form(id: &str, secret: &str) -> SetupForm {
    SetupForm {
        client_id: id.into(),
        client_secret: secret.into(),
    }
}

// ---- the setup screens --------------------------------------------------------------------------

#[test]
fn onedrive_and_dropbox_have_a_connect_button_and_a_guide() {
    assert_eq!(provider_named("OneDrive").unwrap().kind, Kind::OneDrive);
    assert_eq!(provider_named("Dropbox").unwrap().kind, Kind::Dropbox);
    assert_eq!(PROVIDERS.len(), 3);

    let steps = ONEDRIVE.steps().join("\n");
    for needed in [
        "App registrations",
        "http://127.0.0.1",
        "Files.ReadWrite",
        "offline_access",
        "personal Microsoft accounts",
        "Allow public client flows",
    ] {
        assert!(steps.contains(needed), "OneDrive guide lacks {needed}");
    }
    assert!(ONEDRIVE.note().contains("personal Microsoft account"));
    assert!(ONEDRIVE.note().contains("organization"));
    assert!(!ONEDRIVE.needs_secret() && !DROPBOX.needs_secret() && GOOGLE_DRIVE.needs_secret());

    let steps = DROPBOX.steps().join("\n");
    for scope in (DROPBOX.config)().scopes {
        assert!(steps.contains(&scope), "Dropbox guide lacks {scope}");
    }
    for needed in [
        "Scoped access",
        "App folder",
        "Full Dropbox",
        "http://127.0.0.1",
    ] {
        assert!(steps.contains(needed), "Dropbox guide lacks {needed}");
    }
    let choices: Vec<bool> = PROVIDERS.iter().map(|p| p.access_choice).collect();
    assert_eq!(choices, [false, false, true]);
    // The scopes the guide names are the scopes the sign-in asks for.
    for scope in ["Files.ReadWrite", "offline_access"] {
        assert!((ONEDRIVE.config)().scopes.iter().any(|s| s == scope));
    }
}

#[test]
fn client_ids_are_checked_in_each_providers_shape_and_no_secret_is_taken() {
    let ok = validate_setup(&ONEDRIVE, &form(&format!(" {MS_ID}\n"), "")).unwrap();
    assert_eq!(ok.client_id, MS_ID);
    assert!(ok.client_secret.is_none());
    let ok = validate_setup(&DROPBOX, &form(DBX_KEY, "")).unwrap();
    assert_eq!(ok.client_id, DBX_KEY);
    assert!(ok.client_secret.is_none());

    for bad in [
        "",
        "my-app",
        "11111111-2222-3333-4444-55555555555",
        "11111111-2222-3333-4444-55555555555g",
        "1111111122223333444455555555555a",
    ] {
        let e = validate_setup(&ONEDRIVE, &form(bad, "")).unwrap_err();
        assert!(e.client_id.is_some(), "{bad:?} was accepted");
    }
    let e = validate_setup(&ONEDRIVE, &form("my-app", "")).unwrap_err();
    assert!(e
        .client_id
        .unwrap()
        .to_string()
        .contains("application (client) ID"));

    for bad in ["", "short", "has space 1234567", "abcd-1234-efgh-567"] {
        assert!(
            validate_setup(&DROPBOX, &form(bad, "")).is_err(),
            "{bad:?} was accepted"
        );
    }
    // A secret nobody asked for would be sent to the provider; it is refused.
    let e = validate_setup(&DROPBOX, &form(DBX_KEY, "oops")).unwrap_err();
    assert!(e
        .client_secret
        .unwrap()
        .to_string()
        .contains("needs no secret"));
    assert!(e.client_id.is_none());
}

#[test]
fn account_keys_carry_the_provider_and_resolve_to_its_key_names() {
    let a = new_account_key(&ONEDRIVE).unwrap();
    let b = new_account_key(&DROPBOX).unwrap();
    assert!(a.starts_with("onedrive-") && b.starts_with("dropbox-"));
    assert_eq!(a.len(), "onedrive-".len() + 8);
    // The secret keys use the cm-auth provider id, not the account prefix.
    assert_eq!((ONEDRIVE.config)().id, "microsoft");
    assert_eq!((DROPBOX.config)().id, "dropbox");
}

// ---- OneDrive -----------------------------------------------------------------------------------

/// STO-2.AC1 for OneDrive: the account shows and a "CaptureMann" folder exists in the drive.
#[tokio::test]
async fn connecting_onedrive_names_the_account_creates_the_folder_and_keeps_secrets_in_the_keychain(
) {
    let (r, graph) = onedrive_rig();
    let phases = Mutex::new(Vec::new());
    let done = connect(&r.env, &ONEDRIVE, ms_creds(), &Target::New, &|p| {
        phases.lock().unwrap().push(p)
    })
    .await
    .unwrap();
    assert_eq!(done.email, "teacher@example.org");
    assert_eq!(done.folder_name, "CaptureMann");
    assert!(done.note.unwrap().text().contains("work or school"));
    assert_eq!(
        *phases.lock().unwrap(),
        [Phase::Browser, Phase::Account, Phase::Folder, Phase::Saving]
    );
    let folders = graph.folders();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].name, "CaptureMann");

    let list = r.env.store.list().unwrap();
    assert_eq!(list.len(), 1);
    let d = &list[0];
    assert_eq!((d.kind, d.name.as_str()), (Kind::OneDrive, "OneDrive"));
    assert!(d.default);
    let acc = d.account.as_ref().unwrap();
    assert!(acc.account.starts_with("onedrive-"));
    assert_eq!(acc.label, "teacher@example.org");
    assert_eq!(acc.folder_id.as_deref(), Some(folders[0].id.as_str()));
    assert_eq!(acc.link_mode, LinkMode::Anyone);
    assert!(!acc.app_folder);

    // Keys: tokens under `oauth.microsoft.<account>`, the client ID next to them, no secret.
    let token_key = format!("oauth.microsoft.{}", acc.account);
    let tokens = r.secrets.get(&token_key).unwrap().unwrap();
    assert!(tokens.contains("tok-1") && tokens.contains("refresh-1"));
    assert!(tokens.contains("teacher@example.org"), "label kept");
    let client = r
        .secrets
        .get(&format!("{token_key}.client"))
        .unwrap()
        .unwrap();
    assert!(client.contains(MS_ID) && !client.contains("client_secret"));

    let file = std::fs::read_to_string(r.config.path().join("destinations.json")).unwrap();
    for secret in ["tok-1", "refresh-1", MS_ID] {
        assert!(!file.contains(secret), "{secret} leaked into {file}");
    }

    // The browser URL: PKCE, loopback redirect, the two scopes, never a secret.
    let url = r.browser.opened.lock().unwrap()[0].clone();
    assert!(url.starts_with("https://login.microsoftonline.com/common/oauth2/v2.0/authorize?"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A"));
    assert!(url.contains("scope=Files.ReadWrite%20offline_access"));
    assert!(!url.contains("client_secret"));
}

#[tokio::test]
async fn a_personal_onedrive_account_is_told_about_the_organization_links() {
    let (r, graph) = onedrive_rig();
    graph.set_drive_type("personal");
    graph.set_account(Some("me@outlook.example"), "me_outlook.example#EXT#");
    let done = connect(&r.env, &ONEDRIVE, ms_creds(), &Target::New, &|_| {})
        .await
        .unwrap();
    assert_eq!(done.email, "me@outlook.example");
    let note = done.note.unwrap().text();
    assert!(note.contains("personal Microsoft account") && note.contains("work or school"));
}

#[tokio::test]
async fn onedrive_without_a_mailbox_is_named_by_its_principal_name() {
    let (r, graph) = onedrive_rig();
    graph.set_account(None, "t.user@school.onmicrosoft.com");
    let done = connect(&r.env, &ONEDRIVE, ms_creds(), &Target::New, &|_| {})
        .await
        .unwrap();
    assert_eq!(done.email, "t.user@school.onmicrosoft.com");
}

#[tokio::test]
async fn a_onedrive_failure_after_the_sign_in_leaves_no_secret_and_no_destination() {
    let (r, graph) = onedrive_rig();
    graph.inject(
        fake_onedrive::Call::FolderGet,
        0,
        Fault::Status {
            status: 403,
            retry_after: None,
            reason: "accessDenied",
            message: "Access denied.",
        },
        1,
    );
    let err = connect(&r.env, &ONEDRIVE, ms_creds(), &Target::New, &|_| {})
        .await
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::PlatformRejected, "{err}");
    assert!(r.env.store.list().unwrap().is_empty());
    assert!(
        format!("{:?}", r.secrets).contains("keys: []"),
        "no half-connected account stays in the keychain"
    );
}

#[tokio::test]
async fn onedrive_refusing_the_code_leaves_nothing_behind() {
    let graph = Arc::new(FakeOneDrive::new());
    let r = rig_for(
        graph,
        &[(
            400,
            r#"{"error":"invalid_client","error_description":"AADSTS700016"}"#,
        )],
    );
    let err = connect(&r.env, &ONEDRIVE, ms_creds(), &Target::New, &|_| {})
        .await
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::PlatformRejected);
    assert!(r.env.store.list().unwrap().is_empty());
    assert!(format!("{:?}", r.secrets).contains("keys: []"));
}

#[tokio::test]
async fn onedrive_uploads_work_through_the_daemons_token_source_and_reconnect_keeps_the_destination(
) {
    let graph = Arc::new(FakeOneDrive::new());
    let r = rig_for(
        graph.clone(),
        &[(200, EXCHANGE), (200, &EXCHANGE.replace("tok-1", "tok-2"))],
    );
    let first = connect(&r.env, &ONEDRIVE, ms_creds(), &Target::New, &|_| {})
        .await
        .unwrap();
    let dest = r.env.store.get(&first.destination_id).unwrap();
    test_connection(&r.env, &dest).await.unwrap();
    assert!(graph.files().is_empty(), "the test file is deleted again");
    assert_eq!(graph.count(fake_onedrive::Call::Delete), 1);

    // The user tunes the destination; the sign-in is gone; they reconnect.
    let mut list = r.env.store.list().unwrap();
    list[0].name = "School OneDrive".into();
    list[0].account.as_mut().unwrap().link_mode = LinkMode::Organization;
    r.env.store.save(&list).unwrap();
    graph.set_valid_token("tok-2");
    let again = connect(
        &r.env,
        &ONEDRIVE,
        ms_creds(),
        &Target::Reconnect {
            destination_id: first.destination_id.clone(),
        },
        &|_| {},
    )
    .await
    .unwrap();
    assert!(again.reconnected);
    let list = r.env.store.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "School OneDrive");
    let acc = list[0].account.as_ref().unwrap();
    assert_eq!(acc.link_mode, LinkMode::Organization, "the choice is kept");
    assert_eq!(
        graph.folders().len(),
        1,
        "the folder is found, not duplicated"
    );
    test_connection(&r.env, &list[0]).await.unwrap();
}

#[tokio::test]
async fn a_revoked_onedrive_sign_in_is_auth_expired_when_testing() {
    let graph = Arc::new(FakeOneDrive::new());
    let r = rig_for(
        graph.clone(),
        &[(200, EXCHANGE), (400, r#"{"error":"invalid_grant"}"#)],
    );
    let done = connect(&r.env, &ONEDRIVE, ms_creds(), &Target::New, &|_| {})
        .await
        .unwrap();
    let dest = r.env.store.get(&done.destination_id).unwrap();
    graph.set_valid_token("other");
    let err = test_connection(&r.env, &dest).await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::AuthExpired, "{err}");
}

#[tokio::test]
async fn disconnecting_onedrive_deletes_its_secrets_and_touches_nothing_remote() {
    let (r, graph) = onedrive_rig();
    let done = connect(&r.env, &ONEDRIVE, ms_creds(), &Target::New, &|_| {})
        .await
        .unwrap();
    let dest = r.env.store.get(&done.destination_id).unwrap();
    let account = dest.account.unwrap().account;
    let remote = graph.log().len();
    disconnect(&r.env, &done.destination_id).unwrap();
    assert!(r.env.store.list().unwrap().is_empty());
    assert!(format!("{:?}", r.secrets).contains("keys: []"));
    assert!(r
        .secrets
        .get(&format!("oauth.microsoft.{account}"))
        .unwrap()
        .is_none());
    assert_eq!(graph.log().len(), remote, "no call reached Microsoft");
}

// ---- Dropbox ------------------------------------------------------------------------------------

#[tokio::test]
async fn connecting_dropbox_with_full_access_names_the_account_and_stores_the_choice() {
    let (r, dbx) = dropbox_rig();
    let phases = Mutex::new(Vec::new());
    let done = connect_with(
        &r.env,
        &DROPBOX,
        dbx_creds(),
        &Target::New,
        Options { app_folder: false },
        &|p| phases.lock().unwrap().push(p),
    )
    .await
    .unwrap();
    assert_eq!(done.email, "teacher@school.example");
    assert!(done.note.unwrap().text().contains("top of your Dropbox"));
    assert_eq!(
        *phases.lock().unwrap(),
        [Phase::Browser, Phase::Account, Phase::Saving],
        "Dropbox has no folder step"
    );
    assert!(dbx.violations().is_empty());

    let list = r.env.store.list().unwrap();
    let d = &list[0];
    assert_eq!((d.kind, d.name.as_str()), (Kind::Dropbox, "Dropbox"));
    let acc = d.account.as_ref().unwrap();
    assert!(acc.account.starts_with("dropbox-"));
    assert_eq!(acc.label, "teacher@school.example");
    assert_eq!(acc.folder_id, None);
    assert!(!acc.app_folder);

    let token_key = format!("oauth.dropbox.{}", acc.account);
    let tokens = r.secrets.get(&token_key).unwrap().unwrap();
    assert!(tokens.contains("tok-1") && tokens.contains("refresh-1"));
    let client = r
        .secrets
        .get(&format!("{token_key}.client"))
        .unwrap()
        .unwrap();
    assert!(client.contains(DBX_KEY));
    let file = std::fs::read_to_string(r.config.path().join("destinations.json")).unwrap();
    for secret in ["tok-1", "refresh-1", DBX_KEY] {
        assert!(!file.contains(secret), "{secret} leaked into {file}");
    }

    let url = r.browser.opened.lock().unwrap()[0].clone();
    assert!(url.starts_with("https://www.dropbox.com/oauth2/authorize?"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("token_access_type=offline"));
    assert!(url.contains("account_info.read"));
}

#[tokio::test]
async fn the_app_folder_choice_reaches_the_destination_and_its_uploads() {
    let (r, dbx) = dropbox_rig();
    let done = connect_with(
        &r.env,
        &DROPBOX,
        dbx_creds(),
        &Target::New,
        Options { app_folder: true },
        &|_| {},
    )
    .await
    .unwrap();
    assert!(done.note.unwrap().text().contains("under Apps"));
    let dest = r.env.store.get(&done.destination_id).unwrap();
    assert!(dest.account.as_ref().unwrap().app_folder);
    // The stored choice is what the daemon's provider is built from (and the file records it).
    let file = std::fs::read_to_string(r.config.path().join("destinations.json")).unwrap();
    assert!(file.contains("\"app_folder\": true"), "{file}");
    test_connection(&r.env, &dest).await.unwrap();
    assert!(dbx.files().is_empty(), "the test file is deleted again");
    assert_eq!(dbx.count(fake_dropbox::Call::Delete), 1);
    assert!(dbx.violations().is_empty());
}

#[tokio::test]
async fn only_dropbox_takes_the_app_folder_choice() {
    let (r, _graph) = onedrive_rig();
    connect_with(
        &r.env,
        &ONEDRIVE,
        ms_creds(),
        &Target::New,
        Options { app_folder: true },
        &|_| {},
    )
    .await
    .unwrap();
    let list = r.env.store.list().unwrap();
    assert!(!list[0].account.as_ref().unwrap().app_folder);
}

#[tokio::test]
async fn a_dropbox_failure_after_the_sign_in_leaves_no_secret_and_no_destination() {
    let (r, dbx) = dropbox_rig();
    dbx.inject(
        fake_dropbox::Call::Account,
        0,
        Fault::Status {
            status: 403,
            retry_after: None,
            reason: "missing_scope/..",
            message: "",
        },
        1,
    );
    let err = connect(&r.env, &DROPBOX, dbx_creds(), &Target::New, &|_| {})
        .await
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::PlatformRejected, "{err}");
    assert!(err.to_string().contains("reconnect") || err.to_string().contains("permission"));
    assert!(r.env.store.list().unwrap().is_empty());
    assert!(format!("{:?}", r.secrets).contains("keys: []"));
}

#[tokio::test]
async fn dropbox_reconnect_keeps_the_destination_and_the_account_key() {
    let dbx = Arc::new(FakeDropbox::new());
    let r = rig_for(
        dbx.clone(),
        &[(200, EXCHANGE), (200, &EXCHANGE.replace("tok-1", "tok-2"))],
    );
    let first = connect_with(
        &r.env,
        &DROPBOX,
        dbx_creds(),
        &Target::New,
        Options { app_folder: true },
        &|_| {},
    )
    .await
    .unwrap();
    let mut list = r.env.store.list().unwrap();
    list[0].name = "Team Dropbox".into();
    list[0].what = What::Both;
    list[0].account.as_mut().unwrap().link_mode = LinkMode::Organization;
    r.env.store.save(&list).unwrap();
    let account = list[0].account.as_ref().unwrap().account.clone();
    dbx.set_valid_token("tok-2");

    let again = connect_with(
        &r.env,
        &DROPBOX,
        dbx_creds(),
        &Target::Reconnect {
            destination_id: first.destination_id.clone(),
        },
        Options { app_folder: true },
        &|_| {},
    )
    .await
    .unwrap();
    assert!(again.reconnected);
    let list = r.env.store.list().unwrap();
    assert_eq!(list.len(), 1);
    let d = &list[0];
    assert_eq!((d.name.as_str(), d.what), ("Team Dropbox", What::Both));
    let acc = d.account.as_ref().unwrap();
    assert_eq!(acc.account, account);
    assert_eq!(acc.link_mode, LinkMode::Organization);
    assert!(acc.app_folder);
    assert!(r
        .secrets
        .get(&format!("oauth.dropbox.{account}"))
        .unwrap()
        .unwrap()
        .contains("tok-2"));
}

#[tokio::test]
async fn dropbox_state_and_disconnect_follow_its_key_names() {
    let (r, _dbx) = dropbox_rig();
    let done = connect(&r.env, &DROPBOX, dbx_creds(), &Target::New, &|_| {})
        .await
        .unwrap();
    let dest = r.env.store.get(&done.destination_id).unwrap();
    let account = dest.account.as_ref().unwrap().account.clone();
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

    disconnect(&r.env, &done.destination_id).unwrap();
    assert!(r
        .secrets
        .get(&format!("oauth.dropbox.{account}"))
        .unwrap()
        .is_none());
    assert!(r
        .secrets
        .get(&format!("oauth.dropbox.{account}.client"))
        .unwrap()
        .is_none());
    assert!(matches!(
        account_state(r.secrets.as_ref(), &dest, &none),
        AccountState::Reconnect(m) if m.text().contains("Not signed in")
    ));
}

#[tokio::test]
async fn two_providers_in_one_keychain_do_not_mix_their_accounts() {
    // One keychain, one destination list: OneDrive and Dropbox side by side.
    let graph = Arc::new(FakeOneDrive::new());
    let r = rig_for(
        graph,
        &[(200, EXCHANGE), (200, &EXCHANGE.replace("tok-1", "tok-2"))],
    );
    let a = connect(&r.env, &ONEDRIVE, ms_creds(), &Target::New, &|_| {})
        .await
        .unwrap();
    let dbx = Arc::new(FakeDropbox::new());
    dbx.set_valid_token("tok-2");
    let env2 = Env {
        secrets: r.env.secrets.clone(),
        auth_http: r.env.auth_http.clone(),
        browser: r.env.browser.clone(),
        storage_http: dbx,
        tokens: r.env.tokens.clone(),
        store: DestinationStore::new(r.config.path()),
        timeout: r.env.timeout,
    };
    let b = connect(&env2, &DROPBOX, dbx_creds(), &Target::New, &|_| {})
        .await
        .unwrap();
    let list = r.env.store.list().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list.iter().filter(|d| d.default).count(), 1);
    let da = list.iter().find(|d| d.id == a.destination_id).unwrap();
    let db = list.iter().find(|d| d.id == b.destination_id).unwrap();
    assert_eq!((da.kind, db.kind), (Kind::OneDrive, Kind::Dropbox));
    // Each destination finds the right provider's tokens by its own account key.
    test_connection(&r.env, da).await.unwrap();
    test_connection(&env2, db).await.unwrap();
    disconnect(&r.env, &b.destination_id).unwrap();
    let left = r.env.store.list().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].kind, Kind::OneDrive);
    assert!(r
        .secrets
        .get(&format!(
            "oauth.microsoft.{}",
            da.account.as_ref().unwrap().account
        ))
        .unwrap()
        .is_some());
}
