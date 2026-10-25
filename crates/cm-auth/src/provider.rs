//! Provider endpoints and scopes. The client id (and secret) are the user's own.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConfig {
    /// Stable id used in secret key names: `google`, `microsoft`, `dropbox`.
    pub id: &'static str,
    pub display_name: &'static str,
    pub auth_url: &'static str,
    pub token_url: &'static str,
    pub scopes: Vec<String>,
    /// Extra query parameters on the authorization URL.
    pub extra_auth_params: Vec<(String, String)>,
    /// Whether the provider's token endpoint needs a client secret (Google desktop clients do;
    /// Microsoft public clients and Dropbox PKCE do not).
    pub client_secret_required: bool,
}

fn strings(s: &[&str]) -> Vec<String> {
    s.iter().map(|x| x.to_string()).collect()
}

fn pairs(p: &[(&str, &str)]) -> Vec<(String, String)> {
    p.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

impl ProviderConfig {
    /// Google Drive, scope `drive.file` (non-sensitive). `prompt=consent` makes Google return a
    /// refresh token.
    pub fn google_drive() -> Self {
        Self::google(&["https://www.googleapis.com/auth/drive.file"])
    }

    /// Google YouTube: `youtube.upload` (+ `youtube` for playlists and thumbnails). Same project and
    /// endpoints as Drive.
    pub fn google_youtube() -> Self {
        Self::google(&[
            "https://www.googleapis.com/auth/youtube.upload",
            "https://www.googleapis.com/auth/youtube",
        ])
    }

    fn google(scopes: &[&str]) -> Self {
        Self {
            id: "google",
            display_name: "Google",
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
            token_url: "https://oauth2.googleapis.com/token",
            scopes: strings(scopes),
            extra_auth_params: pairs(&[("access_type", "offline"), ("prompt", "consent")]),
            client_secret_required: true,
        }
    }

    /// Microsoft identity platform (OneDrive via Graph), personal and work accounts.
    pub fn microsoft() -> Self {
        Self {
            id: "microsoft",
            display_name: "Microsoft",
            auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
            scopes: strings(&["Files.ReadWrite", "offline_access"]),
            extra_auth_params: Vec::new(),
            client_secret_required: false,
        }
    }

    /// Dropbox, PKCE without a secret; `token_access_type=offline` returns a refresh token.
    pub fn dropbox() -> Self {
        Self {
            id: "dropbox",
            display_name: "Dropbox",
            auth_url: "https://www.dropbox.com/oauth2/authorize",
            token_url: "https://api.dropboxapi.com/oauth2/token",
            scopes: strings(&[
                "account_info.read",
                "files.content.write",
                "files.metadata.read",
                "sharing.write",
                "sharing.read",
            ]),
            extra_auth_params: pairs(&[("token_access_type", "offline")]),
            client_secret_required: false,
        }
    }
}

/// The user's own app credentials. The secret is optional and hidden from `Debug`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientCredentials {
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
}

impl fmt::Debug for ClientCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientCredentials")
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "[redacted]"),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_match_the_spec() {
        assert_eq!(
            ProviderConfig::google_drive().scopes,
            ["https://www.googleapis.com/auth/drive.file"]
        );
        assert_eq!(
            ProviderConfig::microsoft().scopes,
            ["Files.ReadWrite", "offline_access"]
        );
        let d = ProviderConfig::dropbox();
        assert_eq!(
            d.scopes,
            [
                "account_info.read",
                "files.content.write",
                "files.metadata.read",
                "sharing.write",
                "sharing.read"
            ]
        );
        assert!(d
            .extra_auth_params
            .contains(&("token_access_type".into(), "offline".into())));
        assert!(!d.client_secret_required);
    }

    #[test]
    fn credentials_debug_hides_secret() {
        let c = ClientCredentials {
            client_id: "id-1".into(),
            client_secret: Some("shh-secret".into()),
        };
        let d = format!("{c:?}");
        assert!(d.contains("id-1"));
        assert!(!d.contains("shh-secret"));
    }
}
