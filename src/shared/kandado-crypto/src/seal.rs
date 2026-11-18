//! Sealed boxes: encrypt to someone's public key without identifying the sender.
//!
//! Used to give a vault key to each member and an item key to each share
//! recipient. This is HPKE (RFC 9180) in base mode with the suite
//! DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, ChaCha20-Poly1305, used single-shot.
//!
//! Blob layout: `version (1) || encapsulated key (32) || ciphertext || tag (16)`.
//!
//! A sealed box does not say who made it. Anyone with the public key can seal
//! to it, so what is inside must be judged on its own merits.

use core::fmt;

use hpke::{
    Deserializable, Kem as _, OpModeR, OpModeS, Serializable, aead::ChaCha20Poly1305,
    kdf::HkdfSha256, kem::X25519HkdfSha256,
};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::{Context, Error, Result, SecretBytes};

type Kem = X25519HkdfSha256;
type HpkePublicKey = <Kem as hpke::Kem>::PublicKey;
type HpkePrivateKey = <Kem as hpke::Kem>::PrivateKey;
type HpkeEncappedKey = <Kem as hpke::Kem>::EncappedKey;

/// Suite 1: HPKE base mode, X25519 / HKDF-SHA256 / ChaCha20-Poly1305.
pub const VERSION: u8 = 1;
pub const PUBLIC_KEY_LEN: usize = 32;
pub const PRIVATE_KEY_LEN: usize = 32;
const ENCAPPED_LEN: usize = 32;
const TAG_LEN: usize = 16;
/// Bytes a sealed blob is longer than its plaintext.
pub const OVERHEAD: usize = 1 + ENCAPPED_LEN + TAG_LEN;

/// HPKE `info`: separates Kandado sealed boxes from any other use of the key.
const INFO: &[u8] = b"kandado/seal/v1";

/// An X25519 public key. Safe to store, send and show.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PublicKey([u8; PUBLIC_KEY_LEN]);

impl PublicKey {
    pub fn from_bytes(bytes: [u8; PUBLIC_KEY_LEN]) -> Self {
        Self(bytes)
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        bytes.try_into().map(Self).map_err(|_| Error::InvalidKey)
    }

    pub fn as_bytes(&self) -> &[u8; PUBLIC_KEY_LEN] {
        &self.0
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey(")?;
        for byte in &self.0[..4] {
            write!(f, "{byte:02x}")?;
        }
        write!(f, "..)")
    }
}

/// An X25519 private key. Wiped from memory when dropped; cannot be printed.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct PrivateKey([u8; PRIVATE_KEY_LEN]);

impl PrivateKey {
    pub fn from_bytes(bytes: [u8; PRIVATE_KEY_LEN]) -> Self {
        Self(bytes)
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let mut key = Self([0; PRIVATE_KEY_LEN]);
        if bytes.len() != PRIVATE_KEY_LEN {
            return Err(Error::InvalidKey);
        }
        key.0.copy_from_slice(bytes);
        Ok(key)
    }

    /// The raw key bytes, for encrypting under the user key. Never log or store them in the clear.
    pub fn expose(&self) -> &[u8; PRIVATE_KEY_LEN] {
        &self.0
    }

    /// The public key that belongs to this private key.
    pub fn public_key(&self) -> Result<PublicKey> {
        let private = self.to_hpke()?;
        Ok(public_from_hpke(&Kem::sk_to_pk(&private)))
    }

    fn to_hpke(&self) -> Result<HpkePrivateKey> {
        HpkePrivateKey::from_bytes(&self.0).map_err(|_| Error::InvalidKey)
    }
}

impl fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrivateKey(..)")
    }
}

fn public_from_hpke(public: &HpkePublicKey) -> PublicKey {
    let mut bytes = [0; PUBLIC_KEY_LEN];
    public.write_exact(&mut bytes);
    PublicKey(bytes)
}

/// Creates a new random key pair.
pub fn generate_keypair() -> Result<(PrivateKey, PublicKey)> {
    let mut seed = Zeroizing::new([0u8; PRIVATE_KEY_LEN]);
    crate::random::fill(seed.as_mut_slice())?;
    let (private, public) = Kem::derive_keypair(seed.as_slice());
    let mut bytes = Zeroizing::new([0u8; PRIVATE_KEY_LEN]);
    private.write_exact(bytes.as_mut_slice());
    Ok((PrivateKey(*bytes), public_from_hpke(&public)))
}

/// Encrypts `plaintext` so that only the holder of `recipient`'s private key
/// can read it, bound to `context`.
pub fn seal(recipient: &PublicKey, context: &Context, plaintext: &[u8]) -> Result<Vec<u8>> {
    let recipient = HpkePublicKey::from_bytes(&recipient.0).map_err(|_| Error::InvalidKey)?;
    let (encapped, ciphertext) = hpke::single_shot_seal::<ChaCha20Poly1305, HkdfSha256, Kem>(
        &OpModeS::Base,
        &recipient,
        INFO,
        plaintext,
        &context.to_bytes(),
    )
    .map_err(|_| Error::InvalidKey)?;

    let mut blob = Vec::with_capacity(1 + ENCAPPED_LEN + ciphertext.len());
    blob.push(VERSION);
    blob.extend_from_slice(&encapped.to_bytes());
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

/// Decrypts a blob made by [`seal`] for this private key and context.
pub fn open(recipient: &PrivateKey, context: &Context, blob: &[u8]) -> Result<SecretBytes> {
    let (&version, rest) = blob.split_first().ok_or(Error::Format)?;
    if version != VERSION {
        return Err(Error::UnsupportedVersion(version));
    }
    if rest.len() < ENCAPPED_LEN + TAG_LEN {
        return Err(Error::Format);
    }
    let (encapped, ciphertext) = rest.split_at(ENCAPPED_LEN);
    let encapped = HpkeEncappedKey::from_bytes(encapped).map_err(|_| Error::Decrypt)?;
    let private = recipient.to_hpke()?;
    hpke::single_shot_open::<ChaCha20Poly1305, HkdfSha256, Kem>(
        &OpModeR::Base,
        &private,
        &encapped,
        INFO,
        ciphertext,
        &context.to_bytes(),
    )
    .map(Zeroizing::new)
    .map_err(|_| Error::Decrypt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Purpose, SecretKey};

    fn context() -> Context {
        Context::new(Purpose::VaultKey, [1; 16], [2; 16], 1)
    }

    #[test]
    fn round_trips_and_adds_fixed_overhead() {
        let (private, public) = generate_keypair().unwrap();
        let vault_key = SecretKey::generate().unwrap();
        let blob = seal(&public, &context(), vault_key.expose()).unwrap();
        assert_eq!(blob.len(), vault_key.expose().len() + OVERHEAD);
        assert_eq!(
            open(&private, &context(), &blob).unwrap().as_slice(),
            vault_key.expose()
        );
    }

    #[test]
    fn the_public_key_can_be_recomputed_from_the_private_key() {
        let (private, public) = generate_keypair().unwrap();
        assert_eq!(private.public_key().unwrap(), public);
        let restored = PrivateKey::from_slice(private.expose()).unwrap();
        assert_eq!(restored.public_key().unwrap(), public);
    }

    #[test]
    fn two_seals_of_the_same_plaintext_differ() {
        let (_, public) = generate_keypair().unwrap();
        assert_ne!(
            seal(&public, &context(), b"same").unwrap(),
            seal(&public, &context(), b"same").unwrap()
        );
    }

    #[test]
    fn only_the_recipient_can_open() {
        let (_, public) = generate_keypair().unwrap();
        let (other_private, _) = generate_keypair().unwrap();
        let blob = seal(&public, &context(), b"secret").unwrap();
        assert_eq!(
            open(&other_private, &context(), &blob).unwrap_err(),
            Error::Decrypt
        );
    }

    #[test]
    fn rejects_a_different_context() {
        let (private, public) = generate_keypair().unwrap();
        let blob = seal(&public, &context(), b"secret").unwrap();
        let other = Context {
            object: [3; 16],
            ..context()
        };
        assert_eq!(open(&private, &other, &blob).unwrap_err(), Error::Decrypt);
    }

    #[test]
    fn rejects_every_single_bit_flip() {
        let (private, public) = generate_keypair().unwrap();
        let blob = seal(&public, &context(), b"secret").unwrap();
        for index in 1..blob.len() {
            let mut tampered = blob.clone();
            tampered[index] ^= 1;
            assert_eq!(
                open(&private, &context(), &tampered).unwrap_err(),
                Error::Decrypt
            );
        }
    }

    #[test]
    fn rejects_unknown_versions_and_short_blobs() {
        let (private, public) = generate_keypair().unwrap();
        let mut blob = seal(&public, &context(), b"secret").unwrap();
        blob[0] = 9;
        assert_eq!(
            open(&private, &context(), &blob).unwrap_err(),
            Error::UnsupportedVersion(9)
        );
        assert_eq!(open(&private, &context(), &[]).unwrap_err(), Error::Format);
        assert_eq!(
            open(&private, &context(), &[VERSION; OVERHEAD - 1]).unwrap_err(),
            Error::Format
        );
    }

    #[test]
    fn debug_output_never_shows_the_private_key() {
        let (private, _) = generate_keypair().unwrap();
        assert_eq!(format!("{private:?}"), "PrivateKey(..)");
    }
}
