//! The YouTube connect flow against fakes: a fake browser that plays the user, a scripted
//! sign-in server, a scripted YouTube API and an in-memory secret store. No network, no
//! keychain, no browser.

#![allow(clippy::unwrap_used)]

use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use cm_auth::{HttpError, HttpResponse, MemoryStore};
use cm_core::ErrorCode;
use cm_storage::http::{HttpReply, HttpRequest, TransportError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::*;

const GOOD_ID: &str = "1234-abcd.apps.googleusercontent.com";
const SECRET: &str = "GOCSPX-client-secret-value";
const EXCHANGE: &str = r#"{"access_token":"tok-1","refresh_token":"refresh-1","expires_in":3600}"#;
const REFRESH: &str = r#"{"access_token":"tok-2","expires_in":3600}"#;
const CHANNEL: &str = r#"{"items":[{"id":"UC123","snippet":{"title":"Teacher Tips"}}]}"#;

/// Plays the user: reads `redirect_uri` and `state` from the URL, then calls the loopback back.
struct FakeBrowser {
    opened: Mutex<Vec<String>>,
    /// `false`: nobody answers (the user closes the tab).
    answer: bool,
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
struct SignIn {
    answers: Mutex<VecDeque<(u16, String)>>,
    /// Every request: the URL and the form fields.
    calls: Mutex<Vec<FormCall>>,
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

/// YouTube's API: answers `channels.list` in order and records the bearer tokens it saw.
struct FakeYouTube {
    answers: Mutex<VecDeque<(u16, String)>>,
    bearers: Mutex<Vec<String>>,
}

#[async_trait]
impl StorageHttp for FakeYouTube {
    async fn send(&self, req: HttpRequest) -> Result<HttpReply, TransportError> {
        assert!(req.url.contains("/youtube/v3/channels"), "{}", req.url);
        assert!(req.url.contains("mine=true"));
        self.bearers
            .lock()
            .unwrap()
            .push(req.header_value("authorization").unwrap_or("").to_owned());
        let (status, body) = self
            .answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or((500, String::new()));
        Ok(HttpReply::new(status).body(body))
    }
}

struct Rig {
    env: Env,
    signin: Arc<SignIn>,
    youtube: Arc<FakeYouTube>,
    secrets: Arc<MemoryStore>,
    browser: Arc<FakeBrowser>,
    config: tempfile::TempDir,
}

fn rig_with(sign_in: &[(u16, &str)], channels: &[(u16, &str)], user_answers: bool) -> Rig {
    let config = tempfile::tempdir().unwrap();
    let secrets = Arc::new(MemoryStore::new());
    let queue = |a: &[(u16, &str)]| -> Mutex<VecDeque<(u16, String)>> {
        Mutex::new(a.iter().map(|(s, b)| (*s, (*b).to_owned())).collect())
    };
    let signin = Arc::new(SignIn {
        answers: queue(sign_in),
        calls: Mutex::new(Vec::new()),
    });
    let youtube = Arc::new(FakeYouTube {
        answers: queue(channels),
        bearers: Mutex::new(Vec::new()),
    });
    let browser = Arc::new(FakeBrowser {
        opened: Mutex::new(Vec::new()),
        answer: user_answers,
    });
    let env = Env {
        secrets: secrets.clone(),
        auth_http: signin.clone(),
        browser: browser.clone(),
        http: youtube.clone(),
        tokens: KeychainTokens::new(secrets.clone(), signin.clone()),
        store: PublishAccountStore::new(config.path()),
        timeout: Duration::from_secs(5),
    };
    Rig {
        signin,
        env,
        youtube,
        secrets,
        browser,
        config,
    }
}

fn rig() -> Rig {
    rig_with(&[(200, EXCHANGE)], &[(200, CHANNEL)], true)
}

fn creds() -> ClientCredentials {
    ClientCredentials {
        client_id: GOOD_ID.into(),
        client_secret: Some(SECRET.into()),
    }
}

async fn connect_new(r: &Rig) -> Result<Connected, CmError> {
    connect(&r.env, creds(), &Target::New, &|_| {}).await
}

// ---- the setup screen ---------------------------------------------------------------------------

#[test]
fn setup_values_are_checked_before_anything_happens() {
    let ok = validate_setup(&SetupForm {
        client_id: format!("  {GOOD_ID}\n"),
        client_secret: format!(" {SECRET} "),
    })
    .unwrap();
    assert_eq!(ok.client_id, GOOD_ID);
    assert_eq!(ok.client_secret.as_deref(), Some(SECRET));

    let e = validate_setup(&SetupForm::default()).unwrap_err();
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

    let e = validate_setup(&SetupForm {
        client_id: "my-project-id".into(),
        client_secret: SECRET.into(),
    })
    .unwrap_err();
    assert!(e
        .client_id
        .unwrap()
        .to_string()
        .contains("apps.googleusercontent.com"));
    assert!(e.client_secret.is_none());

    let e = validate_setup(&SetupForm {
        client_id: format!("{GOOD_ID} extra"),
        client_secret: "a b".into(),
    })
    .unwrap_err();
    assert!(e.client_id.unwrap().to_string().contains("no spaces"));
    assert!(e.client_secret.unwrap().to_string().contains("no spaces"));
}

#[test]
fn the_guide_names_the_api_the_unverified_screen_and_the_limits() {
    let steps = YOUTUBE_GUIDE.steps().join(" ");
    assert!(steps.contains("YouTube Data API v3"));
    assert!(steps.contains("Desktop app"));
    let notes = YOUTUBE_GUIDE.notes().join(" ");
    assert!(notes.contains("hasn\u{2019}t verified"));
    assert!(notes.contains("test users"));
    assert!(notes.contains("7 days"));
    assert!(notes.contains("100"));
    assert!(notes.contains("private"));
}

#[test]
fn account_keys_are_unique_and_safe_in_a_key_name() {
    let a = new_account_key().unwrap();
    let b = new_account_key().unwrap();
    assert_ne!(a, b);
    assert!(a.starts_with("youtube-") && a.len() == "youtube-".len() + 8);
    assert!(a
        .chars()
        .all(|c| c == '-' || c.is_ascii_lowercase() || c.is_ascii_digit()));
}

// ---- connecting ---------------------------------------------------------------------------------

/// PUB-1.AC1: the channel name appears and the token is in the keychain, not in any file.
#[tokio::test]
async fn connecting_stores_secrets_in_the_keychain_and_only_key_names_in_the_file() {
    let r = rig();
    let phases = Mutex::new(Vec::new());
    let done = connect(&r.env, creds(), &Target::New, &|p| {
        phases.lock().unwrap().push(p)
    })
    .await
    .unwrap();
    assert_eq!(done.channel_name, "Teacher Tips");
    assert!(!done.reconnected && !done.channel_changed);
    assert_eq!(
        *phases.lock().unwrap(),
        [Phase::Browser, Phase::Channel, Phase::Saving]
    );

    let list = r.env.store.list().unwrap();
    assert_eq!(list.len(), 1);
    let a = &list[0];
    assert_eq!(a.id, done.account_id);
    assert_eq!(a.platform, "youtube");
    assert_eq!(a.name, "Teacher Tips");
    assert_eq!(a.account_id, "UC123");
    assert_eq!(a.default_privacy, Privacy::Unlisted);

    let token_key = format!("oauth.google.{}", a.id);
    let tokens = r.secrets.get(&token_key).unwrap().unwrap();
    assert!(tokens.contains("tok-1") && tokens.contains("refresh-1"));
    assert!(tokens.contains("Teacher Tips"), "label kept");
    let client = r
        .secrets
        .get(&format!("{token_key}.client"))
        .unwrap()
        .unwrap();
    assert!(client.contains(GOOD_ID) && client.contains(SECRET));

    let file = std::fs::read_to_string(r.config.path().join("publish_accounts.json")).unwrap();
    for secret in ["tok-1", "refresh-1", SECRET, GOOD_ID] {
        assert!(!file.contains(secret), "{secret} leaked into {file}");
    }

    // The browser URL carried PKCE and both YouTube scopes, never the secret; the channel call
    // used the new token.
    let url = r.browser.opened.lock().unwrap()[0].clone();
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("youtube.upload") && url.contains("auth%2Fyoutube"));
    assert!(!url.contains(SECRET));
    assert_eq!(r.youtube.bearers.lock().unwrap()[0], "Bearer tok-1");
}

#[tokio::test]
async fn a_second_account_is_a_second_entry() {
    let r = rig_with(
        &[(200, EXCHANGE), (200, EXCHANGE)],
        &[
            (200, CHANNEL),
            (
                200,
                r#"{"items":[{"id":"UC9","snippet":{"title":"Second"}}]}"#,
            ),
        ],
        true,
    );
    let a = connect_new(&r).await.unwrap();
    let b = connect_new(&r).await.unwrap();
    assert_ne!(a.account_id, b.account_id);
    assert_eq!(r.env.store.list().unwrap().len(), 2);
}

#[tokio::test]
async fn a_user_who_never_answers_leaves_nothing_behind() {
    let mut r = rig_with(&[(200, EXCHANGE)], &[], false);
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
async fn google_refusing_the_code_leaves_nothing_behind() {
    let r = rig_with(
        &[(
            400,
            r#"{"error":"invalid_client","error_description":"bad"}"#,
        )],
        &[],
        true,
    );
    let err = connect_new(&r).await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::PlatformRejected);
    assert!(r.env.store.list().unwrap().is_empty());
}

#[tokio::test]
async fn an_account_without_a_channel_removes_the_new_secrets_and_adds_no_account() {
    let r = rig_with(&[(200, EXCHANGE)], &[(200, r#"{"items":[]}"#)], true);
    let err = connect_new(&r).await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::PlatformRejected);
    assert!(err.to_string().contains("no YouTube channel"));
    assert!(r.env.store.list().unwrap().is_empty());
    assert!(format!("{:?}", r.secrets).contains("keys: []"));
}

#[tokio::test]
async fn the_api_not_being_enabled_is_explained() {
    let r = rig_with(
        &[(200, EXCHANGE)],
        &[(
            403,
            r#"{"error":{"message":"off","errors":[{"reason":"accessNotConfigured"}]}}"#,
        )],
        true,
    );
    let err = connect_new(&r).await.unwrap_err();
    assert!(err
        .to_string()
        .contains("YouTube Data API v3 is not enabled"));
    assert!(r.env.store.list().unwrap().is_empty());
}

// ---- test connection, state, reconnect, disconnect ------------------------------------------------

#[tokio::test]
async fn test_connection_renews_the_token_and_lists_the_channel() {
    let r = rig_with(
        &[(200, EXCHANGE), (200, REFRESH)],
        &[(200, CHANNEL), (200, CHANNEL)],
        true,
    );
    let done = connect_new(&r).await.unwrap();
    let ch = test_connection(&r.env, &done.account_id).await.unwrap();
    assert_eq!(ch.title, "Teacher Tips");
    // The second channel call used the renewed token.
    assert_eq!(r.youtube.bearers.lock().unwrap()[1], "Bearer tok-2");
}

#[tokio::test]
async fn a_refused_renewal_is_auth_expired_and_the_card_can_offer_reconnect() {
    let r = rig_with(
        &[
            (200, EXCHANGE),
            (
                400,
                r#"{"error":"invalid_grant","error_description":"revoked"}"#,
            ),
        ],
        &[(200, CHANNEL)],
        true,
    );
    let done = connect_new(&r).await.unwrap();
    let err = test_connection(&r.env, &done.account_id).await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::AuthExpired);
}

#[tokio::test]
async fn account_state_follows_the_keychain() {
    let r = rig();
    let done = connect_new(&r).await.unwrap();
    let a = r.env.store.get(&done.account_id).unwrap();
    assert_eq!(
        account_state(r.secrets.as_ref(), &a),
        AccountState::Connected
    );
    r.secrets.delete(&format!("oauth.google.{}", a.id)).unwrap();
    assert!(matches!(
        account_state(r.secrets.as_ref(), &a),
        AccountState::Reconnect(m) if m.text().contains("Not signed in")
    ));
}

#[tokio::test]
async fn reconnect_keeps_the_account_key_and_its_default_privacy() {
    let r = rig_with(
        &[(200, EXCHANGE), (200, EXCHANGE)],
        &[
            (200, CHANNEL),
            (
                200,
                r#"{"items":[{"id":"UC999","snippet":{"title":"Other"}}]}"#,
            ),
        ],
        true,
    );
    let first = connect_new(&r).await.unwrap();
    set_default_privacy(&r.env, &first.account_id, Privacy::Private).unwrap();
    let again = connect(
        &r.env,
        creds(),
        &Target::Reconnect {
            account_id: first.account_id.clone(),
        },
        &|_| {},
    )
    .await
    .unwrap();
    assert!(again.reconnected && again.channel_changed);
    assert_eq!(again.account_id, first.account_id);
    let list = r.env.store.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "Other");
    assert_eq!(list[0].default_privacy, Privacy::Private);
}

#[tokio::test]
async fn a_failed_reconnect_keeps_the_account() {
    let r = rig_with(&[(200, EXCHANGE)], &[(200, CHANNEL)], true);
    let first = connect_new(&r).await.unwrap();
    // The next sign-in is refused by Google.
    let signin = Arc::new(SignIn {
        answers: Mutex::new(VecDeque::from([(
            400,
            r#"{"error":"access_denied"}"#.to_owned(),
        )])),
        calls: Mutex::new(Vec::new()),
    });
    let env = Env {
        secrets: r.env.secrets.clone(),
        auth_http: signin,
        browser: r.env.browser.clone(),
        http: r.env.http.clone(),
        tokens: r.env.tokens.clone(),
        store: r.env.store.clone(),
        timeout: r.env.timeout,
    };
    let err = connect(
        &env,
        creds(),
        &Target::Reconnect {
            account_id: first.account_id.clone(),
        },
        &|_| {},
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), ErrorCode::PlatformRejected);
    assert_eq!(r.env.store.list().unwrap().len(), 1);
}

#[tokio::test]
async fn disconnect_deletes_the_secrets_and_the_entry_and_nothing_else() {
    let r = rig();
    let done = connect_new(&r).await.unwrap();
    let calls_before = r.youtube.bearers.lock().unwrap().len();
    disconnect(&r.env, &done.account_id).unwrap();
    assert!(r.env.store.list().unwrap().is_empty());
    assert!(format!("{:?}", r.secrets).contains("keys: []"));
    // Nothing was revoked or called at Google.
    assert_eq!(r.youtube.bearers.lock().unwrap().len(), calls_before);
    assert_eq!(
        disconnect(&r.env, &done.account_id).unwrap_err().code(),
        ErrorCode::NotFound
    );
}

/// PUB-1.AC5: Disconnect asks Google to revoke the refresh token before the secrets go.
#[tokio::test]
async fn disconnect_revokes_the_refresh_token_at_google_then_forgets_the_account() {
    let r = rig_with(&[(200, EXCHANGE), (200, "{}")], &[(200, CHANNEL)], true);
    let done = connect_new(&r).await.unwrap();
    let outcome = disconnect_and_revoke(&r.env, &done.account_id)
        .await
        .unwrap();
    assert_eq!(outcome, RevokeOutcome::Revoked);
    let calls = r.signin.calls.lock().unwrap();
    let (url, form) = calls.last().unwrap();
    assert_eq!(url, "https://oauth2.googleapis.com/revoke");
    assert_eq!(form, &vec![("token".to_owned(), "refresh-1".to_owned())]);
    assert!(r.env.store.list().unwrap().is_empty());
    assert!(format!("{:?}", r.secrets).contains("keys: []"));
    let msg = sign_out_message("Teacher Tips", &outcome);
    assert!(msg.contains("Google was told"), "{msg}");
}

/// Offline: the account is still signed out here, and the words say how to finish at Google.
#[tokio::test]
async fn a_failed_revoke_still_disconnects_and_says_where_to_finish() {
    // The token endpoint has nothing more to say: the revoke gets its default 500.
    let r = rig_with(&[(200, EXCHANGE)], &[(200, CHANNEL)], true);
    let done = connect_new(&r).await.unwrap();
    let outcome = disconnect_and_revoke(&r.env, &done.account_id)
        .await
        .unwrap();
    assert!(outcome.needs_manual_removal(), "{outcome:?}");
    assert!(r.env.store.list().unwrap().is_empty());
    assert!(format!("{:?}", r.secrets).contains("keys: []"));
    let msg = sign_out_message("Teacher Tips", &outcome);
    assert!(msg.contains("signed out here"), "{msg}");
    assert!(msg.contains("myaccount.google.com/permissions"), "{msg}");
    assert!(!msg.contains("refresh-1") && !msg.contains(SECRET));
}

#[tokio::test]
async fn disconnecting_an_unknown_account_sends_nothing() {
    let r = rig();
    let before = r.signin.calls.lock().unwrap().len();
    let err = disconnect_and_revoke(&r.env, "youtube-nope")
        .await
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::NotFound);
    assert_eq!(r.signin.calls.lock().unwrap().len(), before);
}

#[tokio::test]
async fn several_channels_connect_side_by_side_and_one_becomes_the_default() {
    let other = r#"{"items":[{"id":"UC999","snippet":{"title":"Second Channel"}}]}"#;
    let r = rig_with(
        &[(200, EXCHANGE), (200, EXCHANGE)],
        &[(200, CHANNEL), (200, other)],
        true,
    );
    let a = connect_new(&r).await.unwrap();
    let b = connect_new(&r).await.unwrap();
    assert_ne!(a.account_id, b.account_id);
    let names = |r: &Rig| -> Vec<String> {
        r.env
            .store
            .list()
            .unwrap()
            .into_iter()
            .map(|x| x.name)
            .collect()
    };
    assert_eq!(names(&r), ["Teacher Tips", "Second Channel"]);
    make_default(&r.env, &b.account_id).unwrap();
    assert_eq!(names(&r), ["Second Channel", "Teacher Tips"]);
    // Disconnecting one leaves the other's secrets alone.
    disconnect(&r.env, &b.account_id).unwrap();
    let kept = r.env.store.get(&a.account_id).unwrap();
    assert_eq!(
        account_state(r.secrets.as_ref(), &kept),
        AccountState::Connected
    );
}

#[test]
fn the_card_detail_names_the_channel_and_the_default_visibility() {
    let a = PublishAccount {
        id: "youtube-x".into(),
        platform: "youtube".into(),
        name: "Teacher Tips".into(),
        account_id: "UC1".into(),
        default_privacy: Privacy::Unlisted,
    };
    assert_eq!(
        card_detail(&a),
        "Teacher Tips \u{b7} default visibility: Unlisted"
    );
}

/// I18N P2: the guide's steps and notes are built when drawn.
#[test]
fn guide_texts_follow_the_language() {
    use cm_i18n::{with_language, Lang};
    let (steps, notes) = (YOUTUBE_GUIDE.steps(), YOUTUBE_GUIDE.notes());
    assert_eq!(steps.len(), 5);
    assert_eq!(notes.len(), 2);
    let pseudo = with_language(Lang::EnXa, || YOUTUBE_GUIDE.steps());
    assert_ne!(steps[0], pseudo[0]);
    assert_eq!(privacy_label(Privacy::Unlisted), "Unlisted");
    for lang in [Lang::Vi, Lang::EnXa] {
        with_language(lang, || {
            let m = sign_out_message("Mine", &RevokeOutcome::NotAttempted);
            assert!(m.contains("Mine") && !m.contains("{name}"), "{lang:?}: {m}");
        });
    }
}
