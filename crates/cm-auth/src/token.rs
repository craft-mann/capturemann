//! Token storage (one JSON secret per account) and refresh on demand.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use cm_core::CmError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::http::HttpClient;
use crate::provider::{ClientCredentials, ProviderConfig};
use crate::safe_text;
use crate::secret::SecretStore;

/// A token is refreshed when it expires within this many seconds.
pub const EXPIRY_SKEW_SECS: i64 = 60;
const DEFAULT_LIFETIME_SECS: i64 = 3600;

/// What is stored for one account. `Debug` never prints the tokens.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Unix seconds.
    pub expires_at: i64,
    #[serde(default)]
    pub scopes: Vec<String>,
    /// For example the account email; shown in Settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_label: Option<String>,
}

impl TokenSet {
    pub fn is_expired(&self, now: i64) -> bool {
        now + EXPIRY_SKEW_SECS >= self.expires_at
    }
}

impl fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenSet")
            .field("access_token", &"[redacted]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[redacted]"),
            )
            .field("expires_at", &self.expires_at)
            .field("scopes", &self.scopes)
            .field("account_label", &self.account_label)
            .finish()
    }
}

pub(crate) fn system_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// POSTs to the token endpoint and maps the outcome to the shared error codes.
///
/// `refresh` selects the meaning of `invalid_grant`: for a refresh it is a revoked or expired
/// connection (`auth_expired`); for a code exchange it is a bad code (`platform_rejected`).
/// Error text holds the provider's error code and scrubbed description, never a token.
pub(crate) async fn request_tokens(
    http: &dyn HttpClient,
    token_url: &str,
    form: &[(String, String)],
    refresh: bool,
    now: i64,
) -> Result<TokenSet, CmError> {
    let resp = http.post_form(token_url, form).await.map_err(|e| {
        CmError::network(format!(
            "Could not reach the sign-in server: {}",
            safe_text(&e.0)
        ))
    })?;
    let json: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
    if (200..300).contains(&resp.status) {
        let Some(access) = json["access_token"].as_str().filter(|s| !s.is_empty()) else {
            return Err(CmError::platform_rejected(
                "The sign-in server's answer had no access token.",
                None,
            ));
        };
        let lifetime = match &json["expires_in"] {
            Value::Number(n) => n.as_i64(),
            Value::String(s) => s.parse().ok(),
            _ => None,
        }
        .unwrap_or(DEFAULT_LIFETIME_SECS);
        return Ok(TokenSet {
            access_token: access.to_string(),
            refresh_token: json["refresh_token"].as_str().map(str::to_string),
            expires_at: now + lifetime,
            scopes: json["scope"]
                .as_str()
                .map(|s| {
                    s.split([' ', ','])
                        .filter(|x| !x.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            account_label: None,
        });
    }
    let code = json["error"].as_str().map(safe_text);
    if refresh && code.as_deref() == Some("invalid_grant") {
        return Err(CmError::auth_expired(
            "The connection was revoked or has expired. Connect the account again.",
        ));
    }
    if resp.status >= 500 || resp.status == 429 {
        return Err(CmError::network(format!(
            "The sign-in server is not available right now (HTTP {}).",
            resp.status
        )));
    }
    let desc = json["error_description"].as_str().map(safe_text);
    let text = match (&code, desc) {
        (Some(c), Some(d)) => format!("The provider rejected the request: {c}: {d}"),
        (Some(c), None) => format!("The provider rejected the request: {c}"),
        _ => format!("The provider rejected the request (HTTP {}).", resp.status),
    };
    Err(CmError::platform_rejected(text, code))
}

/// Holds the tokens of a provider's accounts and keeps them fresh.
pub struct TokenManager {
    provider: ProviderConfig,
    creds: ClientCredentials,
    http: Arc<dyn HttpClient>,
    store: Arc<dyn SecretStore>,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl TokenManager {
    pub fn new(
        provider: ProviderConfig,
        creds: ClientCredentials,
        http: Arc<dyn HttpClient>,
        store: Arc<dyn SecretStore>,
    ) -> Self {
        Self {
            provider,
            creds,
            http,
            store,
            clock: Arc::new(system_now),
            locks: Mutex::new(HashMap::new()),
        }
    }

    /// Replaces the clock (Unix seconds), for tests.
    pub fn with_clock(mut self, clock: impl Fn() -> i64 + Send + Sync + 'static) -> Self {
        self.clock = Arc::new(clock);
        self
    }

    /// The key name of an account's secret. This (not the value) is what config files keep.
    pub fn secret_key(&self, account: &str) -> String {
        format!("oauth.{}.{account}", self.provider.id)
    }

    pub fn save(&self, account: &str, tokens: &TokenSet) -> Result<(), CmError> {
        let json = serde_json::to_string(tokens)
            .map_err(|e| CmError::internal(format!("could not encode the tokens: {e}")))?;
        self.store.set(&self.secret_key(account), &json)
    }

    pub fn load(&self, account: &str) -> Result<Option<TokenSet>, CmError> {
        let Some(json) = self.store.get(&self.secret_key(account))? else {
            return Ok(None);
        };
        serde_json::from_str(&json)
            .map(Some)
            // The stored text is a secret: say nothing about its content.
            .map_err(|_| {
                CmError::internal(
                    "The stored sign-in data is unreadable; connect the account again.",
                )
            })
    }

    /// Removes the account's tokens (disconnect). Not an error when there are none.
    pub fn forget(&self, account: &str) -> Result<(), CmError> {
        self.store.delete(&self.secret_key(account))
    }

    fn lock_for(&self, account: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut m = self.locks.lock().unwrap_or_else(|e| e.into_inner());
        m.entry(account.to_string()).or_default().clone()
    }

    /// A valid access token for `account`, refreshing it first when it expires within 60 s.
    /// Concurrent callers for one account share a single refresh.
    ///
    /// Errors: `auth_expired` when the account is not connected, has no refresh token, or the
    /// refresh was refused as `invalid_grant` (needs reconnect); `network` for transport failures,
    /// 5xx and 429; `platform_rejected` (with `platform_code`) for other 4xx.
    pub async fn access_token(&self, account: &str) -> Result<String, CmError> {
        self.token(account, None).await
    }

    /// A token for `account` that is not `rejected`, refreshing even when the stored one has not
    /// expired by its clock (the server answered 401, so it was revoked or the clock is off). Same
    /// errors and the same per-account serialisation as [`access_token`](Self::access_token):
    /// callers that were rejected at the same moment share one refresh.
    pub async fn force_refresh(&self, account: &str) -> Result<String, CmError> {
        let rejected = self.load(account)?.map(|t| t.access_token);
        self.token(account, Some(rejected)).await
    }

    /// `force`: `None` is the normal path (refresh when near expiry); `Some(rejected)` refreshes
    /// unless another caller already replaced the rejected token while this one waited.
    async fn token(&self, account: &str, force: Option<Option<String>>) -> Result<String, CmError> {
        let not_connected =
            || CmError::auth_expired("This account is not connected. Connect it again.");
        let usable = |t: &TokenSet| match &force {
            None => !t.is_expired((self.clock)()),
            Some(rejected) => rejected.as_deref() != Some(t.access_token.as_str()),
        };
        let tokens = self.load(account)?.ok_or_else(not_connected)?;
        if force.is_none() && usable(&tokens) {
            return Ok(tokens.access_token);
        }
        let lock = self.lock_for(account);
        let _guard = lock.lock().await;
        // Another caller may have refreshed while this one waited.
        let tokens = self.load(account)?.ok_or_else(not_connected)?;
        if usable(&tokens) {
            return Ok(tokens.access_token);
        }
        let Some(refresh_token) = tokens.refresh_token.clone() else {
            return Err(CmError::auth_expired(
                "The connection has expired and cannot be renewed. Connect the account again.",
            ));
        };
        let mut form = vec![
            ("grant_type".to_string(), "refresh_token".to_string()),
            ("refresh_token".to_string(), refresh_token.clone()),
            ("client_id".to_string(), self.creds.client_id.clone()),
        ];
        if let Some(secret) = &self.creds.client_secret {
            form.push(("client_secret".to_string(), secret.clone()));
        }
        let mut fresh = request_tokens(
            self.http.as_ref(),
            self.provider.token_url,
            &form,
            true,
            (self.clock)(),
        )
        .await?;
        // Providers may omit these on refresh: keep what we had.
        if fresh.refresh_token.is_none() {
            fresh.refresh_token = Some(refresh_token);
        }
        if fresh.scopes.is_empty() {
            fresh.scopes = tokens.scopes;
        }
        fresh.account_label = tokens.account_label;
        self.save(account, &fresh)?;
        Ok(fresh.access_token)
    }
}

impl fmt::Debug for TokenManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenManager")
            .field("provider", &self.provider.id)
            .field("creds", &self.creds)
            .finish_non_exhaustive()
    }
}

/// A scripted [`HttpClient`] shared by the tests of this crate.
#[cfg(test)]
pub(crate) mod fake {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use async_trait::async_trait;

    use super::*;
    use crate::http::{HttpError, HttpResponse};

    pub type Call = (String, Vec<(String, String)>);

    pub struct FakeHttp {
        pub responses: Mutex<VecDeque<Result<HttpResponse, HttpError>>>,
        pub calls: Mutex<Vec<Call>>,
        pub count: AtomicUsize,
        pub delay: Duration,
    }

    impl FakeHttp {
        pub fn new(responses: Vec<Result<HttpResponse, HttpError>>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(responses.into()),
                calls: Mutex::new(Vec::new()),
                count: AtomicUsize::new(0),
                delay: Duration::ZERO,
            })
        }

        pub fn with_delay(
            responses: Vec<Result<HttpResponse, HttpError>>,
            delay: Duration,
        ) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(responses.into()),
                calls: Mutex::new(Vec::new()),
                count: AtomicUsize::new(0),
                delay,
            })
        }

        pub fn calls(&self) -> usize {
            self.count.load(Ordering::SeqCst)
        }
    }

    pub fn ok(body: &str) -> Result<HttpResponse, HttpError> {
        status(200, body)
    }

    pub fn status(status: u16, body: &str) -> Result<HttpResponse, HttpError> {
        Ok(HttpResponse {
            status,
            body: body.to_string(),
        })
    }

    #[async_trait]
    impl HttpClient for FakeHttp {
        async fn post_form(
            &self,
            url: &str,
            form: &[(String, String)],
        ) -> Result<HttpResponse, HttpError> {
            self.count.fetch_add(1, Ordering::SeqCst);
            self.calls
                .lock()
                .unwrap()
                .push((url.to_string(), form.to_vec()));
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err(HttpError("no scripted response".into())))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::time::Duration;

    use cm_core::ErrorCode;

    use super::fake::*;
    use super::*;
    use crate::http::HttpError;
    use crate::secret::MemoryStore;

    const OLD_ACCESS: &str = "old-access-token-AAA";
    const REFRESH: &str = "refresh-token-RRR";
    const NEW_ACCESS: &str = "new-access-token-NNN";

    fn creds() -> ClientCredentials {
        ClientCredentials {
            client_id: "cid".into(),
            client_secret: Some("client-secret-SSS".into()),
        }
    }

    fn manager(http: Arc<FakeHttp>, now: Arc<AtomicI64>) -> (Arc<TokenManager>, Arc<MemoryStore>) {
        let store = Arc::new(MemoryStore::new());
        let m = TokenManager::new(ProviderConfig::google_drive(), creds(), http, store.clone())
            .with_clock(move || now.load(Ordering::SeqCst));
        m.save(
            "me",
            &TokenSet {
                access_token: OLD_ACCESS.into(),
                refresh_token: Some(REFRESH.into()),
                expires_at: 1_000,
                scopes: vec!["s1".into()],
                account_label: Some("me@example.com".into()),
            },
        )
        .unwrap();
        (Arc::new(m), store)
    }

    fn refreshed() -> String {
        format!(r#"{{"access_token":"{NEW_ACCESS}","expires_in":3600}}"#)
    }

    #[tokio::test]
    async fn fresh_token_is_returned_without_a_request() {
        let http = FakeHttp::new(vec![]);
        let (m, _) = manager(http.clone(), Arc::new(AtomicI64::new(900)));
        assert_eq!(m.access_token("me").await.unwrap(), OLD_ACCESS);
        assert_eq!(http.calls(), 0);
    }

    #[tokio::test]
    async fn token_inside_the_60_second_skew_is_refreshed() {
        let http = FakeHttp::new(vec![ok(&refreshed())]);
        // 1000 - 60 = 940: at 940 the token counts as expired.
        let (m, store) = manager(http.clone(), Arc::new(AtomicI64::new(940)));
        assert_eq!(m.access_token("me").await.unwrap(), NEW_ACCESS);
        assert_eq!(http.calls(), 1);
        let form = http.calls.lock().unwrap()[0].1.clone();
        assert!(form.contains(&("grant_type".into(), "refresh_token".into())));
        assert!(form.contains(&("refresh_token".into(), REFRESH.into())));
        assert!(form.contains(&("client_secret".into(), "client-secret-SSS".into())));
        // Stored: new access token, old refresh token and label kept, expiry moved.
        let saved = m.load("me").unwrap().unwrap();
        assert_eq!(saved.access_token, NEW_ACCESS);
        assert_eq!(saved.refresh_token.as_deref(), Some(REFRESH));
        assert_eq!(saved.account_label.as_deref(), Some("me@example.com"));
        assert_eq!(saved.expires_at, 940 + 3600);
        assert_eq!(saved.scopes, vec!["s1".to_string()]);
        assert!(store.get("oauth.google.me").unwrap().is_some());
    }

    #[tokio::test]
    async fn rotated_refresh_token_is_stored() {
        let body = format!(
            r#"{{"access_token":"{NEW_ACCESS}","refresh_token":"rotated-RRR2","expires_in":"60","scope":"a b"}}"#
        );
        let http = FakeHttp::new(vec![ok(&body)]);
        let (m, _) = manager(http, Arc::new(AtomicI64::new(5_000)));
        m.access_token("me").await.unwrap();
        let saved = m.load("me").unwrap().unwrap();
        assert_eq!(saved.refresh_token.as_deref(), Some("rotated-RRR2"));
        assert_eq!(saved.scopes, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(saved.expires_at, 5_060);
    }

    #[tokio::test]
    async fn invalid_grant_is_auth_expired_without_secrets_in_the_text() {
        let http = FakeHttp::new(vec![status(
            400,
            &format!(
                r#"{{"error":"invalid_grant","error_description":"Token revoked {REFRESH}"}}"#
            ),
        )]);
        let (m, _) = manager(http, Arc::new(AtomicI64::new(5_000)));
        let err = m.access_token("me").await.unwrap_err();
        assert_eq!(err.code(), ErrorCode::AuthExpired);
        assert_no_secrets(&format!("{err} {err:?}"));
    }

    #[tokio::test]
    async fn server_errors_and_transport_failures_are_network() {
        for resp in [
            status(503, "<html>down</html>"),
            status(429, "{}"),
            Err(HttpError("connection refused".into())),
        ] {
            let http = FakeHttp::new(vec![resp]);
            let (m, _) = manager(http, Arc::new(AtomicI64::new(5_000)));
            let err = m.access_token("me").await.unwrap_err();
            assert_eq!(err.code(), ErrorCode::Network);
            assert!(err.is_retryable());
            assert_no_secrets(&err.to_string());
        }
    }

    #[tokio::test]
    async fn other_4xx_is_platform_rejected_with_the_platform_code() {
        let http = FakeHttp::new(vec![status(
            400,
            r#"{"error":"invalid_client","error_description":"Unknown client"}"#,
        )]);
        let (m, _) = manager(http, Arc::new(AtomicI64::new(5_000)));
        let err = m.access_token("me").await.unwrap_err();
        assert_eq!(err.code(), ErrorCode::PlatformRejected);
        assert_eq!(
            err.data().unwrap()["platform_code"].as_str(),
            Some("invalid_client")
        );
    }

    #[tokio::test]
    async fn missing_account_or_refresh_token_is_auth_expired() {
        let http = FakeHttp::new(vec![]);
        let (m, _) = manager(http.clone(), Arc::new(AtomicI64::new(5_000)));
        assert_eq!(
            m.access_token("nobody").await.unwrap_err().code(),
            ErrorCode::AuthExpired
        );
        let mut t = m.load("me").unwrap().unwrap();
        t.refresh_token = None;
        m.save("me", &t).unwrap();
        assert_eq!(
            m.access_token("me").await.unwrap_err().code(),
            ErrorCode::AuthExpired
        );
        assert_eq!(http.calls(), 0);
    }

    #[tokio::test]
    async fn concurrent_callers_refresh_once() {
        let http = FakeHttp::with_delay(vec![ok(&refreshed())], Duration::from_millis(100));
        let (m, _) = manager(http.clone(), Arc::new(AtomicI64::new(5_000)));
        let tasks: Vec<_> = (0..8)
            .map(|_| {
                let m = m.clone();
                tokio::spawn(async move { m.access_token("me").await })
            })
            .collect();
        for t in tasks {
            assert_eq!(t.await.unwrap().unwrap(), NEW_ACCESS);
        }
        assert_eq!(http.calls(), 1);
    }

    #[tokio::test]
    async fn force_refresh_refreshes_a_token_that_has_not_expired() {
        let http = FakeHttp::new(vec![ok(&refreshed())]);
        // 900 < 1000 - 60: the stored token is still valid by the clock.
        let (m, _) = manager(http.clone(), Arc::new(AtomicI64::new(900)));
        assert_eq!(m.access_token("me").await.unwrap(), OLD_ACCESS);
        assert_eq!(http.calls(), 0);
        assert_eq!(m.force_refresh("me").await.unwrap(), NEW_ACCESS);
        assert_eq!(http.calls(), 1);
        assert_eq!(m.load("me").unwrap().unwrap().access_token, NEW_ACCESS);
        // The refreshed token is the normal answer from now on.
        assert_eq!(m.access_token("me").await.unwrap(), NEW_ACCESS);
        assert_eq!(http.calls(), 1);
    }

    #[tokio::test]
    async fn force_refresh_maps_errors_like_access_token() {
        let http = FakeHttp::new(vec![
            status(400, r#"{"error":"invalid_grant"}"#),
            status(503, "down"),
            status(400, r#"{"error":"invalid_client"}"#),
        ]);
        let (m, _) = manager(http, Arc::new(AtomicI64::new(900)));
        let codes = [
            ErrorCode::AuthExpired,
            ErrorCode::Network,
            ErrorCode::PlatformRejected,
        ];
        for want in codes {
            assert_eq!(m.force_refresh("me").await.unwrap_err().code(), want);
        }
        assert_eq!(
            m.force_refresh("nobody").await.unwrap_err().code(),
            ErrorCode::AuthExpired
        );
    }

    #[tokio::test]
    async fn concurrent_force_refreshes_share_one_request() {
        let http = FakeHttp::with_delay(vec![ok(&refreshed())], Duration::from_millis(100));
        let (m, _) = manager(http.clone(), Arc::new(AtomicI64::new(900)));
        let tasks: Vec<_> = (0..8)
            .map(|_| {
                let m = m.clone();
                tokio::spawn(async move { m.force_refresh("me").await })
            })
            .collect();
        for t in tasks {
            assert_eq!(t.await.unwrap().unwrap(), NEW_ACCESS);
        }
        assert_eq!(http.calls(), 1);
    }

    #[tokio::test]
    async fn forget_removes_the_secret() {
        let http = FakeHttp::new(vec![]);
        let (m, store) = manager(http, Arc::new(AtomicI64::new(0)));
        assert_eq!(m.secret_key("me"), "oauth.google.me");
        m.forget("me").unwrap();
        assert!(store.get("oauth.google.me").unwrap().is_none());
        assert!(m.load("me").unwrap().is_none());
    }

    #[test]
    fn debug_output_never_shows_secrets() {
        let t = TokenSet {
            access_token: OLD_ACCESS.into(),
            refresh_token: Some(REFRESH.into()),
            expires_at: 1,
            scopes: vec![],
            account_label: None,
        };
        assert_no_secrets(&format!("{t:?} {t:#?}"));
        let http = FakeHttp::new(vec![]);
        let (m, _) = manager(http, Arc::new(AtomicI64::new(0)));
        assert_no_secrets(&format!("{m:?}"));
        let r = crate::http::HttpResponse {
            status: 200,
            body: format!("{OLD_ACCESS}{REFRESH}"),
        };
        assert_no_secrets(&format!("{r:?}"));
    }

    #[test]
    fn unreadable_stored_data_does_not_leak() {
        let http = FakeHttp::new(vec![]);
        let (m, store) = manager(http, Arc::new(AtomicI64::new(0)));
        store
            .set("oauth.google.me", "not json REFRESH-LEAK")
            .unwrap();
        let err = m.load("me").unwrap_err();
        assert!(!err.to_string().contains("REFRESH-LEAK"));
    }

    pub(crate) fn assert_no_secrets(text: &str) {
        for s in [OLD_ACCESS, REFRESH, NEW_ACCESS, "client-secret-SSS"] {
            assert!(!text.contains(s), "secret {s} found in {text}");
        }
    }
}
