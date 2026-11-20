//! CSPRNG tokens, PKCE, and token hashing.
//!
//! Everything here is 256 bits of OS randomness encoded base64url without
//! padding, so the values are URL-safe without further escaping.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::TryRng;
use sha2::{Digest, Sha256};

/// Raw OS randomness for values that need a custom encoding (e.g. user codes).
pub fn random_bytes(len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    rand::rngs::SysRng
        .try_fill_bytes(&mut bytes)
        .expect("OS RNG is unavailable");
    bytes
}

/// Random 32-byte value as base64url (43 chars). Used for `state`, `nonce`,
/// PKCE verifiers, session ids, and `su_` tokens.
pub fn random_token() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes(32))
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// `BASE64URL(SHA256(ASCII(verifier)))` — RFC 7636 S256.
pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(sha256(verifier.as_bytes()))
}

/// Lookup key for a session token. The token itself is never stored.
pub fn token_hash(token: &str) -> [u8; 32] {
    sha256(token.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_url_safe_and_distinct() {
        let a = random_token();
        let b = random_token();
        assert_eq!(a.len(), 43);
        assert_ne!(a, b);
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }

    /// RFC 7636 appendix B test vector.
    #[test]
    fn pkce_s256_matches_the_rfc_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            pkce_challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn token_hash_is_stable_and_not_the_token() {
        let hash = token_hash("su_example");
        assert_eq!(hash, token_hash("su_example"));
        assert_ne!(hash, token_hash("su_example2"));
        assert_eq!(hash.len(), 32);
    }
}
