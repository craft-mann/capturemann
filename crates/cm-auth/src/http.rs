//! The HTTP seam: the token endpoint calls go through [`HttpClient`] so tests use a fake.

use std::time::Duration;

use async_trait::async_trait;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

/// A transport failure (no response): DNS, connect, TLS, timeout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpError(pub String);

#[derive(Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl std::fmt::Debug for HttpResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The body of a token response holds tokens.
        f.debug_struct("HttpResponse")
            .field("status", &self.status)
            .field("body", &format_args!("[{} bytes]", self.body.len()))
            .finish()
    }
}

#[async_trait]
pub trait HttpClient: Send + Sync {
    /// POST an `application/x-www-form-urlencoded` body. Any HTTP status is a response, not an error.
    async fn post_form(
        &self,
        url: &str,
        form: &[(String, String)],
    ) -> Result<HttpResponse, HttpError>;
}

/// `ureq` with the system TLS stack (Security.framework, schannel), configured like cm-ai's model
/// download: ureq defaults to Rustls and panics on the first `https://` request when only
/// `native-tls-no-default` is enabled, so the provider is set explicitly.
pub struct UreqClient {
    agent: ureq::Agent,
}

impl UreqClient {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_global(Some(Duration::from_secs(60)))
            .tls_config(
                TlsConfig::builder()
                    .provider(TlsProvider::NativeTls)
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .build()
            .into();
        Self { agent }
    }
}

impl Default for UreqClient {
    fn default() -> Self {
        Self::new()
    }
}

/// `application/x-www-form-urlencoded` encoding.
pub(crate) fn form_encode(form: &[(String, String)]) -> String {
    form.iter()
        .map(|(k, v)| format!("{}={}", crate::flow::encode(k), crate::flow::encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

#[async_trait]
impl HttpClient for UreqClient {
    async fn post_form(
        &self,
        url: &str,
        form: &[(String, String)],
    ) -> Result<HttpResponse, HttpError> {
        let agent = self.agent.clone();
        let url = url.to_string();
        let body = form_encode(form);
        tokio::task::spawn_blocking(move || {
            let mut resp = agent
                .post(&url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .header("Accept", "application/json")
                .send(body)
                .map_err(|e| HttpError(crate::safe_text(&e.to_string())))?;
            let status = resp.status().as_u16();
            let body = resp
                .body_mut()
                .read_to_string()
                .map_err(|e| HttpError(crate::safe_text(&e.to_string())))?;
            Ok(HttpResponse { status, body })
        })
        .await
        .map_err(|e| HttpError(format!("http task: {e}")))?
    }
}
