//! Passkey keys: ES256, which is ECDSA over P-256 with SHA-256 (COSE algorithm
//! -7). The one algorithm every relying party accepts.
//!
//! A [`PasskeyKey`] is made from the system random source, stored inside the
//! passkey item as PKCS #8 DER, and used only to sign. It cannot be printed and
//! is wiped from memory when dropped (the signing key zeroizes itself).
//! Signatures are deterministic (RFC 6979), so signing needs no randomness.

use core::fmt;

use p256::{
    ecdsa::{DerSignature, SigningKey, signature::Signer as _},
    pkcs8::{DecodePrivateKey as _, EncodePrivateKey as _, EncodePublicKey as _},
};
use zeroize::Zeroizing;

use crate::{Error, Result, random};

/// COSE algorithm identifier of ES256.
pub const ES256: i64 = -7;

/// A passkey's private key.
pub struct PasskeyKey(SigningKey);

impl PasskeyKey {
    /// A new random key.
    pub fn generate() -> Result<Self> {
        // Almost every 32 random bytes are a valid scalar; retry the rare
        // value at or above the group order.
        for _ in 0..8 {
            let mut bytes = Zeroizing::new([0u8; 32]);
            random::fill(&mut *bytes)?;
            if let Ok(key) = SigningKey::from_slice(&*bytes) {
                return Ok(Self(key));
            }
        }
        Err(Error::Random)
    }

    /// Reads a key stored as PKCS #8 DER.
    pub fn from_pkcs8_der(der: &[u8]) -> Result<Self> {
        SigningKey::from_pkcs8_der(der)
            .map(Self)
            .map_err(|_| Error::InvalidKey)
    }

    /// The key as PKCS #8 DER, for storing in the passkey item.
    pub fn to_pkcs8_der(&self) -> Result<Zeroizing<Vec<u8>>> {
        let document = self.0.to_pkcs8_der().map_err(|_| Error::InvalidKey)?;
        Ok(Zeroizing::new(document.as_bytes().to_vec()))
    }

    /// The public key's affine coordinates `(x, y)`, for the COSE key.
    pub fn public_xy(&self) -> ([u8; 32], [u8; 32]) {
        let point = self.0.verifying_key().to_sec1_point(false);
        let point = point.as_bytes();
        // Uncompressed SEC1: 0x04 || x || y.
        let mut x = [0; 32];
        let mut y = [0; 32];
        x.copy_from_slice(&point[1..33]);
        y.copy_from_slice(&point[33..65]);
        (x, y)
    }

    /// The public key as DER SubjectPublicKeyInfo, which
    /// `AuthenticatorAttestationResponse.getPublicKey()` returns.
    pub fn public_spki_der(&self) -> Result<Vec<u8>> {
        self.0
            .verifying_key()
            .to_public_key_der()
            .map(|document| document.as_bytes().to_vec())
            .map_err(|_| Error::InvalidKey)
    }

    /// Signs `message` (hashed with SHA-256 inside) and returns the signature
    /// in ASN.1 DER, as WebAuthn wants it.
    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        let signature: DerSignature = self.0.sign(message);
        signature.as_bytes().to_vec()
    }
}

impl fmt::Debug for PasskeyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PasskeyKey(..)")
    }
}

#[cfg(test)]
mod tests {
    use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier as _};
    use p256::pkcs8::DecodePublicKey as _;

    use super::*;

    #[test]
    fn signs_what_the_public_key_verifies() {
        let key = PasskeyKey::generate().unwrap();
        let message = b"authenticator data || client data hash";
        let signature = Signature::from_der(&key.sign(message)).unwrap();
        let public = VerifyingKey::from_public_key_der(&key.public_spki_der().unwrap()).unwrap();
        public.verify(message, &signature).unwrap();
        assert!(public.verify(b"something else", &signature).is_err());
    }

    #[test]
    fn the_stored_form_round_trips() {
        let key = PasskeyKey::generate().unwrap();
        let stored = key.to_pkcs8_der().unwrap();
        let again = PasskeyKey::from_pkcs8_der(&stored).unwrap();
        assert_eq!(again.public_xy(), key.public_xy());
        // Deterministic signatures: the same key signs the same message the same way.
        assert_eq!(again.sign(b"m"), key.sign(b"m"));
        assert_eq!(
            PasskeyKey::from_pkcs8_der(b"not a key").unwrap_err(),
            Error::InvalidKey
        );
    }

    #[test]
    fn the_public_key_is_the_spki_point() {
        let key = PasskeyKey::generate().unwrap();
        let (x, y) = key.public_xy();
        let spki = key.public_spki_der().unwrap();
        // The SPKI ends with the uncompressed point.
        assert_eq!(&spki[spki.len() - 65..spki.len() - 64], &[0x04]);
        assert_eq!(&spki[spki.len() - 64..spki.len() - 32], &x);
        assert_eq!(&spki[spki.len() - 32..], &y);
    }

    #[test]
    fn two_keys_differ_and_debug_shows_nothing() {
        let (a, b) = (
            PasskeyKey::generate().unwrap(),
            PasskeyKey::generate().unwrap(),
        );
        assert_ne!(a.public_xy(), b.public_xy());
        assert_eq!(format!("{a:?}"), "PasskeyKey(..)");
    }

    /// RFC 6979 appendix A.2.5, P-256 with SHA-256, message "sample".
    #[test]
    fn matches_the_rfc_6979_vector() {
        let private =
            hex::decode("C9AFA9D845BA75166B5C215767B1D6934E50C3DB36E89B127B8A622B120F6721")
                .unwrap();
        let key = PasskeyKey(SigningKey::from_slice(&private).unwrap());
        let signature = Signature::from_der(&key.sign(b"sample")).unwrap();
        let (r, s) = signature.split_bytes();
        assert_eq!(
            hex::encode_upper(r),
            "EFD48B2AACB6A8FD1140DD9CD45E81D69D2C877B56AAF991C34D0EA84EAF3716"
        );
        // A low-S signature may be the negation of the RFC's s; both are valid.
        let s = hex::encode_upper(s);
        assert!(
            s == "F7CB1C942D657C41D436C7A1B6E29F65F3E900DBB9AFF4064DC4AB2F843ACDA8"
                || s == "0834E36AD29A83BF2BC9385E491D6099C93C3C4A9A2D6A4C6B95E3E2AC8F3B79",
            "{s}"
        );
    }
}
