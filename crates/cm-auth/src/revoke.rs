//! Best-effort revocation of a stored sign-in at the provider (PUB-1.AC5, STO-2). Only Google
//! has a revocation endpoint the product uses; for the other providers nothing is attempted.
//! Never fails the caller: the result says what happened so the UI can tell the user in plain
//! words, and the local sign-out goes ahead either way.

use crate::http::HttpClient;
use crate::provider::ProviderConfig;
use crate::secret::SecretStore;
use crate::token::TokenSet;

/// Google's token revocation endpoint (RFC 7009).
pub const GOOGLE_REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";

/// What happened to the provider side of a sign-out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevokeOutcome {
    /// The provider revoked the sign-in (or said it was already revoked).
    Revoked,
    /// Nothing was sent: no token stored, or a provider without a revoke endpoint here.
    NotAttempted,
    /// The provider could not be reached or refused; the local sign-out still happens.
    Failed(String),
}

impl RevokeOutcome {
    /// Whether the user has to be told that access may still be listed at the provider.
    pub fn needs_manual_removal(&self) -> bool {
        matches!(self, RevokeOutcome::Failed(_))
    }
}

/// Revokes the refresh token (or, without one, the access token) stored for `account`.
/// `invalid_token` means it is already revoked and counts as done. Reads the secret store, so
/// call it off the UI thread.
pub async fn revoke_account(
    http: &dyn HttpClient,
    secrets: &dyn SecretStore,
    provider: &ProviderConfig,
    account: &str,
) -> RevokeOutcome {
    if provider.id != "google" {
        return RevokeOutcome::NotAttempted;
    }
    let key = format!("oauth.{}.{account}", provider.id);
    let stored = match secrets.get(&key) {
        Ok(Some(json)) => serde_json::from_str::<TokenSet>(&json).ok(),
        _ => None,
    };
    let Some(tokens) = stored else {
        return RevokeOutcome::NotAttempted;
    };
    let token = tokens
        .refresh_token
        .filter(|t| !t.is_empty())
        .unwrap_or(tokens.access_token);
    if token.is_empty() {
        return RevokeOutcome::NotAttempted;
    }
    match http
        .post_form(GOOGLE_REVOKE_URL, &[("token".to_owned(), token)])
        .await
    {
        Ok(r) if (200..300).contains(&r.status) => RevokeOutcome::Revoked,
        Ok(r) if r.status == 400 && r.body.contains("invalid_token") => RevokeOutcome::Revoked,
        Ok(r) => RevokeOutcome::Failed(format!("Google answered HTTP {}.", r.status)),
        Err(e) => RevokeOutcome::Failed(crate::safe_text(&e.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpError;
    use crate::secret::MemoryStore;
    use crate::token::fake::{ok, status, FakeHttp};

    fn stored(refresh: Option<&str>) -> MemoryStore {
        let s = MemoryStore::new();
        let t = TokenSet {
            access_token: "access-AAA".into(),
            refresh_token: refresh.map(str::to_owned),
            expires_at: 0,
            scopes: vec![],
            account_label: None,
        };
        s.set("oauth.google.a1", &serde_json::to_string(&t).unwrap())
            .unwrap();
        s
    }

    #[tokio::test]
    async fn revokes_the_refresh_token_at_googles_endpoint() {
        let http = FakeHttp::new(vec![ok("{}")]);
        let out = revoke_account(
            http.as_ref(),
            &stored(Some("refresh-RRR")),
            &ProviderConfig::google_youtube(),
            "a1",
        )
        .await;
        assert_eq!(out, RevokeOutcome::Revoked);
        let calls = http.calls.lock().unwrap();
        assert_eq!(calls[0].0, GOOGLE_REVOKE_URL);
        assert_eq!(
            calls[0].1,
            vec![("token".to_owned(), "refresh-RRR".to_owned())]
        );
    }

    #[tokio::test]
    async fn falls_back_to_the_access_token() {
        let http = FakeHttp::new(vec![ok("")]);
        revoke_account(
            http.as_ref(),
            &stored(None),
            &ProviderConfig::google_drive(),
            "a1",
        )
        .await;
        assert_eq!(http.calls.lock().unwrap()[0].1[0].1, "access-AAA");
    }

    #[tokio::test]
    async fn already_revoked_counts_as_done() {
        let http = FakeHttp::new(vec![status(400, r#"{"error":"invalid_token"}"#)]);
        let out = revoke_account(
            http.as_ref(),
            &stored(Some("r")),
            &ProviderConfig::google_drive(),
            "a1",
        )
        .await;
        assert_eq!(out, RevokeOutcome::Revoked);
    }

    #[tokio::test]
    async fn offline_or_refused_is_a_failure_without_the_token_in_the_text() {
        let http = FakeHttp::new(vec![Err(HttpError("dns error".into())), status(500, "")]);
        let s = stored(Some("refresh-SECRET"));
        let p = ProviderConfig::google_youtube();
        let a = revoke_account(http.as_ref(), &s, &p, "a1").await;
        assert!(a.needs_manual_removal());
        let b = revoke_account(http.as_ref(), &s, &p, "a1").await;
        assert_eq!(b, RevokeOutcome::Failed("Google answered HTTP 500.".into()));
        assert!(!format!("{a:?}{b:?}").contains("SECRET"));
    }

    #[tokio::test]
    async fn nothing_is_sent_without_a_token_or_for_other_providers() {
        let http = FakeHttp::new(vec![]);
        let g = ProviderConfig::google_drive();
        assert_eq!(
            revoke_account(http.as_ref(), &MemoryStore::new(), &g, "a1").await,
            RevokeOutcome::NotAttempted
        );
        assert_eq!(
            revoke_account(
                http.as_ref(),
                &stored(Some("r")),
                &ProviderConfig::dropbox(),
                "a1"
            )
            .await,
            RevokeOutcome::NotAttempted
        );
        assert_eq!(http.calls(), 0);
    }
}
