//! Pairing the browser extension with the desktop app, and handing it the user
//! key ("Unlock with desktop app", E-02 and D-45).
//!
//! ```text
//! extension: pairing key pair (X25519), private half kept by the extension
//!            ──public key──► desktop, which shows code(public key);
//!                            the person checks the extension shows the same code
//! unlock:    desktop seals the user key to that public key (HPKE, `seal`)
//!            ──sealed──► extension opens it with its private half
//! ```
//!
//! Nothing here authenticates the desktop to the extension, and nothing needs
//! to: a user key that is not this account's fails to open the account's
//! private key, so the extension refuses it. What protects the user key is that
//! only the holder of a paired private key can open what the desktop seals, and
//! that a pairing needs the person to confirm the code on both sides.

use sha2::{Digest, Sha256};

use crate::{
    Context, Purpose, Result, SecretKey,
    context::Id,
    seal::{self, PUBLIC_KEY_LEN, PrivateKey, PublicKey},
};

/// Version of the sealed plaintext: one raw 256-bit user key.
pub const FORMAT: u8 = 1;

const CODE_DOMAIN: &[u8] = b"kandado/pairing-code/v1";

fn context(user: Id, pairing: Id) -> Context {
    Context::new(Purpose::UserKeyToExtension, user, pairing, FORMAT)
}

/// The six digits both sides show while pairing, from the extension's public key.
pub fn code(public_key: &[u8; PUBLIC_KEY_LEN]) -> String {
    let digest = Sha256::new()
        .chain_update(CODE_DOMAIN)
        .chain_update(public_key)
        .finalize();
    let number = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) % 1_000_000;
    format!("{number:06}")
}

/// Seals the user key to a paired extension. Run by the desktop app.
pub fn seal_user_key(
    user: Id,
    pairing: Id,
    extension: &PublicKey,
    user_key: &SecretKey,
) -> Result<Vec<u8>> {
    seal::seal(extension, &context(user, pairing), user_key.expose())
}

/// Opens what the desktop sealed. Run by the extension, with its pairing key.
pub fn open_user_key(
    user: Id,
    pairing: Id,
    private: &PrivateKey,
    sealed: &[u8],
) -> Result<SecretKey> {
    let bytes = seal::open(private, &context(user, pairing), sealed)?;
    SecretKey::from_slice(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Error, seal::generate_keypair};

    const USER: Id = [1; 16];
    const PAIRING: Id = [2; 16];

    #[test]
    fn the_code_is_six_digits_and_frozen() {
        // Frozen: the extension (WebAssembly) and the desktop must show the same
        // digits. Checked independently: SHA-256 of the domain and the key, first
        // four bytes big-endian, modulo a million.
        assert_eq!(code(&[0; 32]), "244942");
        assert_eq!(code(&[7; 32]), "521029");
        assert_ne!(code(&[7; 32]), code(&[8; 32]));
    }

    #[test]
    fn the_paired_extension_opens_the_user_key() {
        let (private, public) = generate_keypair().unwrap();
        let user_key = SecretKey::generate().unwrap();
        let sealed = seal_user_key(USER, PAIRING, &public, &user_key).unwrap();
        let opened = open_user_key(USER, PAIRING, &private, &sealed).unwrap();
        assert_eq!(opened.expose(), user_key.expose());
    }

    #[test]
    fn it_is_bound_to_the_account_the_pairing_and_the_key() {
        let (private, public) = generate_keypair().unwrap();
        let (other, _) = generate_keypair().unwrap();
        let user_key = SecretKey::generate().unwrap();
        let sealed = seal_user_key(USER, PAIRING, &public, &user_key).unwrap();
        assert_eq!(
            open_user_key([9; 16], PAIRING, &private, &sealed).unwrap_err(),
            Error::Decrypt
        );
        assert_eq!(
            open_user_key(USER, [9; 16], &private, &sealed).unwrap_err(),
            Error::Decrypt
        );
        assert_eq!(
            open_user_key(USER, PAIRING, &other, &sealed).unwrap_err(),
            Error::Decrypt
        );
    }
}
