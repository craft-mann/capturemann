//! PKCE (RFC 7636), method S256, and the random values around it.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use cm_core::CmError;
use sha2::{Digest, Sha256};

/// `BASE64URL(SHA256(verifier))` without padding.
pub fn challenge_s256(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// A random URL-safe string from `n` random bytes.
pub fn random_token(n: usize) -> Result<String, CmError> {
    let mut buf = vec![0u8; n];
    getrandom::fill(&mut buf).map_err(|e| CmError::internal(format!("random numbers: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(buf))
}

/// A code verifier: 32 random bytes, 43 characters (RFC 7636 allows 43 to 128).
pub fn new_verifier() -> Result<String, CmError> {
    random_token(32)
}

/// The anti-CSRF `state`.
pub fn new_state() -> Result<String, CmError> {
    random_token(16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc7636_appendix_b_vector() {
        assert_eq!(
            challenge_s256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifier_is_43_url_safe_chars_and_unique() {
        let a = new_verifier().unwrap();
        let b = new_verifier().unwrap();
        assert_eq!(a.len(), 43);
        assert_ne!(a, b);
        assert!(a
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }
}
