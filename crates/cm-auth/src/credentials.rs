//! The user's own OAuth app credentials (client ID and, where the provider needs it, the client
//! secret), kept in the secret store under one key per account. Config files hold only the
//! account key; the tokens live under [`TokenManager::secret_key`](crate::TokenManager::secret_key)
//! and the credentials under [`credentials_key`].

use cm_core::CmError;

use crate::provider::ClientCredentials;
use crate::secret::SecretStore;

/// The key name of an account's client credentials: `oauth.<provider>.<account>.client`.
pub fn credentials_key(provider_id: &str, account: &str) -> String {
    format!("oauth.{provider_id}.{account}.client")
}

pub fn save_credentials(
    store: &dyn SecretStore,
    provider_id: &str,
    account: &str,
    creds: &ClientCredentials,
) -> Result<(), CmError> {
    let json = serde_json::to_string(creds)
        .map_err(|e| CmError::internal(format!("could not encode the credentials: {e}")))?;
    store.set(&credentials_key(provider_id, account), &json)
}

pub fn load_credentials(
    store: &dyn SecretStore,
    provider_id: &str,
    account: &str,
) -> Result<Option<ClientCredentials>, CmError> {
    let Some(json) = store.get(&credentials_key(provider_id, account))? else {
        return Ok(None);
    };
    // The stored text is a secret: say nothing about its content.
    serde_json::from_str(&json).map(Some).map_err(|_| {
        CmError::internal(
            "The stored client credentials are unreadable; connect the account again.",
        )
    })
}

pub fn forget_credentials(
    store: &dyn SecretStore,
    provider_id: &str,
    account: &str,
) -> Result<(), CmError> {
    store.delete(&credentials_key(provider_id, account))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::MemoryStore;

    #[test]
    fn round_trip_and_forget() {
        let s = MemoryStore::new();
        assert_eq!(credentials_key("google", "a1"), "oauth.google.a1.client");
        assert!(load_credentials(&s, "google", "a1").unwrap().is_none());
        let c = ClientCredentials {
            client_id: "id".into(),
            client_secret: Some("shh".into()),
        };
        save_credentials(&s, "google", "a1", &c).unwrap();
        assert_eq!(load_credentials(&s, "google", "a1").unwrap(), Some(c));
        forget_credentials(&s, "google", "a1").unwrap();
        forget_credentials(&s, "google", "a1").unwrap();
        assert!(load_credentials(&s, "google", "a1").unwrap().is_none());
        s.set("oauth.google.a1.client", "not json LEAK").unwrap();
        let err = load_credentials(&s, "google", "a1").unwrap_err();
        assert!(!err.to_string().contains("LEAK"));
    }
}
