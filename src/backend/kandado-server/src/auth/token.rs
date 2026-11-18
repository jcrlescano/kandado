//! Tokens and one-time codes.
//!
//! Access tokens are short-lived and signed (HMAC-SHA256), so a request is
//! recognised without a lookup; the session extractor still checks that the
//! device has not been revoked. Refresh tokens and handoff codes are random,
//! opaque and stored only as SHA-256 hashes.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;

/// How long an access token lives.
pub const ACCESS_TOKEN_SECONDS: u32 = 15 * 60;

const PREFIX: &str = "k1.";

type HmacSha256 = Hmac<Sha256>;

/// What a verified access token says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Claims {
    pub user_id: Uuid,
    pub device_id: Uuid,
    /// When the person last signed in with Google.
    pub auth_time: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

#[derive(Serialize, Deserialize)]
struct Wire {
    u: Uuid,
    d: Uuid,
    a: i64,
    e: i64,
}

/// Signs and checks access tokens.
#[derive(Clone)]
pub struct TokenSigner {
    secret: Vec<u8>,
}

impl TokenSigner {
    pub fn new(secret: &[u8]) -> Self {
        Self {
            secret: secret.to_vec(),
        }
    }

    fn mac(&self) -> HmacSha256 {
        HmacSha256::new_from_slice(&self.secret).expect("HMAC accepts any key length")
    }

    pub fn issue(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        auth_time: OffsetDateTime,
        now: OffsetDateTime,
    ) -> String {
        let wire = Wire {
            u: user_id,
            d: device_id,
            a: auth_time.unix_timestamp(),
            e: now.unix_timestamp() + i64::from(ACCESS_TOKEN_SECONDS),
        };
        let payload = URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&wire).expect("a fixed struct always serialises"));
        let mut mac = self.mac();
        mac.update(payload.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        format!("{PREFIX}{payload}.{signature}")
    }

    /// Returns the claims of a token that is correctly signed and not expired.
    pub fn verify(&self, token: &str, now: OffsetDateTime) -> Option<Claims> {
        let rest = token.strip_prefix(PREFIX)?;
        let (payload, signature) = rest.split_once('.')?;
        let signature = URL_SAFE_NO_PAD.decode(signature).ok()?;
        let mut mac = self.mac();
        mac.update(payload.as_bytes());
        // Constant-time comparison.
        mac.verify_slice(&signature).ok()?;
        let wire: Wire = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).ok()?).ok()?;
        if wire.e <= now.unix_timestamp() {
            return None;
        }
        Some(Claims {
            user_id: wire.u,
            device_id: wire.d,
            auth_time: OffsetDateTime::from_unix_timestamp(wire.a).ok()?,
            expires_at: OffsetDateTime::from_unix_timestamp(wire.e).ok()?,
        })
    }
}

/// A fresh random secret (refresh token or handoff code), 256 bits, base64url.
pub fn random_secret() -> Result<String, getrandom::Error> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// What the database stores in place of a secret.
pub fn hash_secret(secret: &str) -> Vec<u8> {
    Sha256::digest(secret.as_bytes()).to_vec()
}

/// Checks a PKCE verifier against the challenge sent at the start.
pub fn challenge_matches(verifier: &str, challenge: &str) -> bool {
    let computed = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    constant_time_eq(computed.as_bytes(), challenge.as_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;

    fn signer() -> TokenSigner {
        TokenSigner::new(b"0123456789abcdef0123456789abcdef")
    }

    fn now() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap()
    }

    #[test]
    fn a_token_round_trips() {
        let (user, device) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let auth_time = now() - Duration::minutes(3);
        let token = signer().issue(user, device, auth_time, now());
        let claims = signer().verify(&token, now()).unwrap();
        assert_eq!(claims.user_id, user);
        assert_eq!(claims.device_id, device);
        assert_eq!(claims.auth_time, auth_time);
    }

    #[test]
    fn an_expired_token_is_refused() {
        let token = signer().issue(Uuid::nil(), Uuid::nil(), now(), now());
        let later = now() + Duration::seconds(i64::from(ACCESS_TOKEN_SECONDS));
        assert!(signer().verify(&token, later).is_none());
        assert!(
            signer()
                .verify(&token, later - Duration::seconds(1))
                .is_some()
        );
    }

    #[test]
    fn a_tampered_or_foreign_token_is_refused() {
        let token = signer().issue(Uuid::nil(), Uuid::nil(), now(), now());
        let other = TokenSigner::new(b"ffffffffffffffffffffffffffffffff");
        assert!(other.verify(&token, now()).is_none());

        // Change one character of the payload.
        let mut bytes = token.clone().into_bytes();
        let at = PREFIX.len() + 2;
        bytes[at] = if bytes[at] == b'A' { b'B' } else { b'A' };
        assert!(
            signer()
                .verify(&String::from_utf8(bytes).unwrap(), now())
                .is_none()
        );

        for junk in ["", "k1.", "k1..", "k1.a.b", "k2.a.b", "nonsense"] {
            assert!(signer().verify(junk, now()).is_none(), "{junk}");
        }
    }

    #[test]
    fn secrets_are_random_and_hash_consistently() {
        let a = random_secret().unwrap();
        let b = random_secret().unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert_eq!(hash_secret(&a), hash_secret(&a));
        assert_ne!(hash_secret(&a), hash_secret(&b));
    }

    #[test]
    fn pkce_verifier_must_match_the_challenge() {
        // RFC 7636 appendix B.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert!(challenge_matches(verifier, challenge));
        assert!(!challenge_matches("another", challenge));
        assert!(!challenge_matches(verifier, ""));
    }
}
