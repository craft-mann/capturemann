//! The authorization-code flow with PKCE (S256) and a loopback redirect.

use std::time::Duration;

use cm_core::CmError;
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

use crate::browser::BrowserOpener;
use crate::http::HttpClient;
use crate::loopback::LoopbackListener;
use crate::pkce;
use crate::provider::{ClientCredentials, ProviderConfig};
use crate::token::{request_tokens, system_now, TokenSet};

/// RFC 3986 unreserved characters stay as they are.
const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

pub(crate) fn encode(s: &str) -> String {
    utf8_percent_encode(s, UNRESERVED).to_string()
}

/// The URL the user opens: `response_type=code`, client id, loopback redirect, scopes, `state`,
/// the S256 challenge, then the provider's extra parameters.
pub fn build_authorization_url(
    provider: &ProviderConfig,
    creds: &ClientCredentials,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> String {
    let mut params: Vec<(&str, String)> = vec![
        ("response_type", "code".to_string()),
        ("client_id", creds.client_id.clone()),
        ("redirect_uri", redirect_uri.to_string()),
        ("scope", provider.scopes.join(" ")),
        ("state", state.to_string()),
        ("code_challenge", code_challenge.to_string()),
        ("code_challenge_method", "S256".to_string()),
    ];
    for (k, v) in &provider.extra_auth_params {
        params.push((k, v.clone()));
    }
    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{}?{query}", provider.auth_url)
}

/// One sign-in: open the browser, wait for the redirect, exchange the code.
pub struct AuthFlow<'a> {
    pub provider: &'a ProviderConfig,
    pub creds: &'a ClientCredentials,
    pub http: &'a dyn HttpClient,
    pub browser: &'a dyn BrowserOpener,
}

impl AuthFlow<'_> {
    /// Runs the flow and returns the tokens (the caller stores them with `TokenManager::save`).
    /// Fails with `invalid_input` when the provider needs a client secret and none was given.
    pub async fn authorize(&self, timeout: Duration) -> Result<TokenSet, CmError> {
        if self.creds.client_id.trim().is_empty() {
            return Err(CmError::invalid_field("client_id", "is empty"));
        }
        if self.provider.client_secret_required && self.creds.client_secret.is_none() {
            return Err(CmError::invalid_field(
                "client_secret",
                format!("{} needs a client secret", self.provider.display_name),
            ));
        }
        let listener = LoopbackListener::bind().await?;
        let redirect_uri = listener.redirect_uri();
        let verifier = pkce::new_verifier()?;
        let state = pkce::new_state()?;
        let url = build_authorization_url(
            self.provider,
            self.creds,
            &redirect_uri,
            &state,
            &pkce::challenge_s256(&verifier),
        );
        self.browser.open(&url)?;
        let code = listener.wait_for_code(&state, timeout).await?;
        self.exchange_code(&code, &verifier, &redirect_uri).await
    }

    /// Trades the authorization code for tokens.
    pub async fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<TokenSet, CmError> {
        let mut form = vec![
            ("grant_type".to_string(), "authorization_code".to_string()),
            ("code".to_string(), code.to_string()),
            ("redirect_uri".to_string(), redirect_uri.to_string()),
            ("client_id".to_string(), self.creds.client_id.clone()),
            ("code_verifier".to_string(), verifier.to_string()),
        ];
        if let Some(secret) = &self.creds.client_secret {
            form.push(("client_secret".to_string(), secret.clone()));
        }
        let mut tokens = request_tokens(
            self.http,
            self.provider.token_url,
            &form,
            false,
            system_now(),
        )
        .await?;
        if tokens.scopes.is_empty() {
            tokens.scopes = self.provider.scopes.clone();
        }
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use cm_core::ErrorCode;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;
    use crate::loopback::parse_query;
    use crate::token::fake::{ok, status, FakeHttp};

    fn creds() -> ClientCredentials {
        ClientCredentials {
            client_id: "my client".into(),
            client_secret: Some("client-secret-SSS".into()),
        }
    }

    fn query_of(url: &str) -> Vec<(String, String)> {
        parse_query(url.split_once('?').unwrap().1)
    }

    fn param(q: &[(String, String)], k: &str) -> String {
        q.iter().find(|(n, _)| n == k).unwrap().1.clone()
    }

    #[test]
    fn authorization_url_has_pkce_state_scopes_and_extras() {
        let p = ProviderConfig::dropbox();
        let url = build_authorization_url(
            &p,
            &creds(),
            "http://127.0.0.1:5555/callback",
            "st-1",
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
        );
        assert!(url.starts_with("https://www.dropbox.com/oauth2/authorize?response_type=code&"));
        assert!(url.contains("client_id=my%20client"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A5555%2Fcallback"));
        assert!(url.contains("scope=account_info.read%20files.content.write%20files.metadata.read%20sharing.write%20sharing.read"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("token_access_type=offline"));
        let q = query_of(&url);
        assert_eq!(param(&q, "state"), "st-1");
        assert_eq!(
            param(&q, "code_challenge"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        // The client secret never goes into the browser URL.
        assert!(!url.contains("client-secret-SSS"));
    }

    #[test]
    fn google_url_asks_for_offline_access() {
        let url = build_authorization_url(
            &ProviderConfig::google_drive(),
            &creds(),
            "http://127.0.0.1:1/callback",
            "s",
            "c",
        );
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        assert!(url.contains("drive.file"));
    }

    /// Plays the user: reads `redirect_uri` and `state` from the opened URL, then calls back.
    struct FakeBrowser {
        opened: Mutex<Vec<String>>,
        /// Overrides the state sent back (to test a mismatch).
        state_override: Option<String>,
        callback: bool,
    }

    impl BrowserOpener for FakeBrowser {
        fn open(&self, url: &str) -> Result<(), CmError> {
            self.opened.lock().unwrap().push(url.to_string());
            if !self.callback {
                return Ok(());
            }
            let q = query_of(url);
            let redirect = param(&q, "redirect_uri");
            let state = self
                .state_override
                .clone()
                .unwrap_or_else(|| param(&q, "state"));
            let port: u16 = redirect
                .split(':')
                .nth(2)
                .unwrap()
                .split('/')
                .next()
                .unwrap()
                .parse()
                .unwrap();
            tokio::spawn(async move {
                let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
                let req = format!(
                    "GET /callback?code=the-code&state={state} HTTP/1.1\r\nHost: x\r\n\r\n"
                );
                s.write_all(req.as_bytes()).await.unwrap();
                let mut out = String::new();
                let _ = s.read_to_string(&mut out).await;
            });
            Ok(())
        }
    }

    fn browser(state_override: Option<&str>, callback: bool) -> FakeBrowser {
        FakeBrowser {
            opened: Mutex::new(vec![]),
            state_override: state_override.map(str::to_string),
            callback,
        }
    }

    const TOKENS: &str =
        r#"{"access_token":"acc-AAA","refresh_token":"ref-RRR","expires_in":3600}"#;

    #[tokio::test]
    async fn full_flow_exchanges_the_code_with_the_verifier() {
        let http = FakeHttp::new(vec![ok(TOKENS)]);
        let b = browser(None, true);
        let provider = ProviderConfig::google_drive();
        let c = creds();
        let flow = AuthFlow {
            provider: &provider,
            creds: &c,
            http: http.as_ref(),
            browser: &b,
        };
        let t = flow.authorize(Duration::from_secs(5)).await.unwrap();
        assert_eq!(t.access_token, "acc-AAA");
        assert_eq!(t.refresh_token.as_deref(), Some("ref-RRR"));
        assert_eq!(t.scopes, provider.scopes);

        let (url, form) = http.calls.lock().unwrap()[0].clone();
        assert_eq!(url, provider.token_url);
        let get = |k: &str| form.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert_eq!(get("grant_type").as_deref(), Some("authorization_code"));
        assert_eq!(get("code").as_deref(), Some("the-code"));
        assert_eq!(get("client_secret").as_deref(), Some("client-secret-SSS"));
        // The verifier sent to the token endpoint matches the challenge in the browser URL.
        let opened = b.opened.lock().unwrap()[0].clone();
        let challenge = param(&query_of(&opened), "code_challenge");
        assert_eq!(
            pkce::challenge_s256(&get("code_verifier").unwrap()),
            challenge
        );
        assert_eq!(
            get("redirect_uri").unwrap(),
            param(&query_of(&opened), "redirect_uri")
        );
    }

    #[tokio::test]
    async fn state_mismatch_stops_before_any_token_request() {
        let http = FakeHttp::new(vec![ok(TOKENS)]);
        let b = browser(Some("forged"), true);
        let provider = ProviderConfig::dropbox();
        let c = ClientCredentials {
            client_id: "id".into(),
            client_secret: None,
        };
        let flow = AuthFlow {
            provider: &provider,
            creds: &c,
            http: http.as_ref(),
            browser: &b,
        };
        let err = flow.authorize(Duration::from_secs(5)).await.unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert_eq!(http.calls(), 0);
    }

    #[tokio::test]
    async fn no_callback_times_out() {
        let http = FakeHttp::new(vec![]);
        let b = browser(None, false);
        let provider = ProviderConfig::dropbox();
        let c = ClientCredentials {
            client_id: "id".into(),
            client_secret: None,
        };
        let flow = AuthFlow {
            provider: &provider,
            creds: &c,
            http: http.as_ref(),
            browser: &b,
        };
        let err = flow
            .authorize(Duration::from_millis(150))
            .await
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::UserDeclined);
    }

    #[tokio::test]
    async fn google_without_a_secret_is_invalid_input() {
        let http = FakeHttp::new(vec![]);
        let b = browser(None, false);
        let provider = ProviderConfig::google_drive();
        let c = ClientCredentials {
            client_id: "id".into(),
            client_secret: None,
        };
        let flow = AuthFlow {
            provider: &provider,
            creds: &c,
            http: http.as_ref(),
            browser: &b,
        };
        let err = flow.authorize(Duration::from_secs(1)).await.unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(b.opened.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn exchange_errors_map_to_shared_codes_without_secrets() {
        let provider = ProviderConfig::microsoft();
        let c = creds();
        let cases = [
            (
                status(
                    400,
                    r#"{"error":"invalid_grant","error_description":"code used"}"#,
                ),
                ErrorCode::PlatformRejected,
            ),
            (status(502, "bad gateway"), ErrorCode::Network),
            (
                Err(crate::http::HttpError("dns failure".into())),
                ErrorCode::Network,
            ),
        ];
        for (resp, expected) in cases {
            let http = FakeHttp::new(vec![resp]);
            let b = browser(None, false);
            let flow = AuthFlow {
                provider: &provider,
                creds: &c,
                http: http.as_ref(),
                browser: &b,
            };
            let err = flow
                .exchange_code("the-code", "verifier-VVV", "http://127.0.0.1:1/callback")
                .await
                .unwrap_err();
            assert_eq!(err.code(), expected);
            let text = format!("{err} {err:?}");
            assert!(!text.contains("verifier-VVV") && !text.contains("the-code"));
            assert!(!text.contains("client-secret-SSS"));
        }
    }
}
