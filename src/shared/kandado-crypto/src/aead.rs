//! Authenticated encryption with XChaCha20-Poly1305.
//!
//! Blob layout: `version (1) || nonce (24) || ciphertext || tag (16)`.
//! The nonce is random; at 192 bits, collisions are not a practical concern
//! for any number of messages under one key.

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use zeroize::Zeroizing;

use crate::{Context, Error, Result, SecretBytes, SecretKey, random};

/// Suite 1: XChaCha20-Poly1305, random 24-byte nonce.
pub const VERSION: u8 = 1;
pub const NONCE_LEN: usize = 24;
pub const TAG_LEN: usize = 16;
/// Bytes a blob is longer than its plaintext.
pub const OVERHEAD: usize = 1 + NONCE_LEN + TAG_LEN;

/// Encrypts `plaintext` under `key`, bound to `context`.
pub fn encrypt(key: &SecretKey, context: &Context, plaintext: &[u8]) -> Result<Vec<u8>> {
    seal_raw(key, &context.to_bytes(), plaintext)
}

/// Decrypts a blob made by [`encrypt`] with the same key and context.
pub fn decrypt(key: &SecretKey, context: &Context, blob: &[u8]) -> Result<SecretBytes> {
    open_raw(key, &context.to_bytes(), blob)
}

/// [`encrypt`] with caller-supplied associated data.
pub(crate) fn seal_raw(key: &SecretKey, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    let mut nonce = [0; NONCE_LEN];
    random::fill(&mut nonce)?;
    seal_with_nonce(key, &nonce, aad, plaintext)
}

fn seal_with_nonce(
    key: &SecretKey,
    nonce: &[u8; NONCE_LEN],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let ciphertext = cipher(key)?
        .encrypt(
            &XNonce::from(*nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| Error::Format)?;
    let mut blob = Vec::with_capacity(1 + NONCE_LEN + ciphertext.len());
    blob.push(VERSION);
    blob.extend_from_slice(nonce);
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

/// [`decrypt`] with caller-supplied associated data.
pub(crate) fn open_raw(key: &SecretKey, aad: &[u8], blob: &[u8]) -> Result<SecretBytes> {
    let (&version, rest) = blob.split_first().ok_or(Error::Format)?;
    if version != VERSION {
        return Err(Error::UnsupportedVersion(version));
    }
    if rest.len() < NONCE_LEN + TAG_LEN {
        return Err(Error::Format);
    }
    let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
    let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| Error::Format)?;
    cipher(key)?
        .decrypt(
            &XNonce::from(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| Error::Decrypt)
}

fn cipher(key: &SecretKey) -> Result<XChaCha20Poly1305> {
    XChaCha20Poly1305::new_from_slice(key.expose()).map_err(|_| Error::InvalidKey)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Purpose, context::NO_ID};

    fn context() -> Context {
        Context::new(Purpose::ItemContent, NO_ID, [7; 16], 1)
    }

    /// draft-irtf-cfrg-xchacha-03, appendix A.3.1.
    #[test]
    fn matches_the_xchacha20_poly1305_reference_vector() {
        let key = SecretKey::from_slice(
            &hex::decode("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f")
                .unwrap(),
        )
        .unwrap();
        let nonce: [u8; NONCE_LEN] =
            hex::decode("404142434445464748494a4b4c4d4e4f5051525354555657")
                .unwrap()
                .try_into()
                .unwrap();
        let aad = hex::decode("50515253c0c1c2c3c4c5c6c7").unwrap();
        let plaintext = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
        let expected = concat!(
            "bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb",
            "731c7f1b0b4aa6440bf3a82f4eda7e39ae64c6708c54c216cb96b72e1213b452",
            "2f8c9ba40db5d945b11b69b982c1bb9e3f3fac2bc369488f76b2383565d3fff9",
            "21f9664c97637da9768812f615c68b13b52e",
            "c0875924c1c7987947deafd8780acf49",
        );

        let blob = seal_with_nonce(&key, &nonce, &aad, plaintext).unwrap();

        assert_eq!(blob[0], VERSION);
        assert_eq!(&blob[1..1 + NONCE_LEN], &nonce);
        assert_eq!(hex::encode(&blob[1 + NONCE_LEN..]), expected);
        assert_eq!(open_raw(&key, &aad, &blob).unwrap().as_slice(), plaintext);
    }

    #[test]
    fn round_trips_and_adds_fixed_overhead() {
        let key = SecretKey::generate().unwrap();
        for plaintext in [&b""[..], b"x", &[0xAB; 4096]] {
            let blob = encrypt(&key, &context(), plaintext).unwrap();
            assert_eq!(blob.len(), plaintext.len() + OVERHEAD);
            assert_eq!(
                decrypt(&key, &context(), &blob).unwrap().as_slice(),
                plaintext
            );
        }
    }

    #[test]
    fn two_encryptions_of_the_same_plaintext_differ() {
        let key = SecretKey::generate().unwrap();
        let a = encrypt(&key, &context(), b"same").unwrap();
        let b = encrypt(&key, &context(), b"same").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn rejects_the_wrong_key() {
        let blob = encrypt(&SecretKey::generate().unwrap(), &context(), b"secret").unwrap();
        let other = SecretKey::generate().unwrap();
        assert_eq!(
            decrypt(&other, &context(), &blob).unwrap_err(),
            Error::Decrypt
        );
    }

    #[test]
    fn rejects_any_change_to_the_context() {
        let key = SecretKey::generate().unwrap();
        let blob = encrypt(&key, &context(), b"secret").unwrap();
        let changed = [
            Context {
                purpose: Purpose::VaultMeta,
                ..context()
            },
            Context {
                owner: [1; 16],
                ..context()
            },
            Context {
                object: [8; 16],
                ..context()
            },
            Context {
                format: 2,
                ..context()
            },
        ];
        for other in changed {
            assert_eq!(decrypt(&key, &other, &blob).unwrap_err(), Error::Decrypt);
        }
    }

    #[test]
    fn rejects_every_single_bit_flip() {
        let key = SecretKey::generate().unwrap();
        let blob = encrypt(&key, &context(), b"secret").unwrap();
        for index in 1..blob.len() {
            let mut tampered = blob.clone();
            tampered[index] ^= 1;
            assert_eq!(
                decrypt(&key, &context(), &tampered).unwrap_err(),
                Error::Decrypt
            );
        }
    }

    #[test]
    fn rejects_unknown_versions_and_short_blobs() {
        let key = SecretKey::generate().unwrap();
        let mut blob = encrypt(&key, &context(), b"secret").unwrap();
        blob[0] = 2;
        assert_eq!(
            decrypt(&key, &context(), &blob).unwrap_err(),
            Error::UnsupportedVersion(2)
        );
        assert_eq!(decrypt(&key, &context(), &[]).unwrap_err(), Error::Format);
        assert_eq!(
            decrypt(&key, &context(), &[VERSION; OVERHEAD - 1]).unwrap_err(),
            Error::Format
        );
    }
}
