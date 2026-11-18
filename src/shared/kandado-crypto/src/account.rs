//! Account keys: how the user key is created, wrapped and recovered.
//!
//! ```text
//! master password ──Argon2id──► master key ──┐
//! recovery phrase ──HKDF──────► recovery key ┴─ each wraps ─► user key (random)
//!                                            user key encrypts ─► X25519 private key
//! ```
//!
//! The server stores an [`AccountBundle`]: wrapped keys and public data it
//! cannot open. Changing the master password re-wraps the user key only, so no
//! vault data is re-encrypted.
//!
//! Signing keys (Ed25519, see `docs/architecture/security.md`) are not created
//! yet; they arrive with the first feature that signs something.

use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

use crate::{
    Context, Error, Purpose, Result, SecretKey,
    context::{Id, NO_ID},
    kdf::{self, KdfParams, SALT_LEN},
    recovery::RecoveryPhrase,
    seal::{self, PRIVATE_KEY_LEN, PUBLIC_KEY_LEN, PrivateKey},
    wrap,
};

/// Version of the bundle layout and of the plaintext inside it.
pub const BUNDLE_VERSION: u8 = 1;

/// Everything the server keeps about a user's keys. None of it opens without
/// the master password or the recovery phrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountBundle {
    pub version: u8,
    pub kdf_salt: [u8; SALT_LEN],
    pub kdf_params: KdfParams,
    pub user_key_by_master: Vec<u8>,
    pub user_key_by_recovery: Vec<u8>,
    pub public_x25519: [u8; PUBLIC_KEY_LEN],
    /// The X25519 private key, encrypted with the user key.
    pub private_keys_enc: Vec<u8>,
}

/// The result of creating an account. The client uploads `bundle`, shows
/// `recovery` once, and keeps `user_key` in memory while unlocked.
pub struct NewAccount {
    pub bundle: AccountBundle,
    pub user_key: SecretKey,
    pub recovery: RecoveryPhrase,
}

/// A re-wrapped user key after a master password change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasterRewrap {
    pub kdf_salt: [u8; SALT_LEN],
    pub kdf_params: KdfParams,
    pub user_key_by_master: Vec<u8>,
}

fn context(purpose: Purpose, user: Id) -> Context {
    Context::new(purpose, user, NO_ID, BUNDLE_VERSION)
}

/// Normalises to Unicode NFKC, so the same password typed on different
/// platforms and keyboards gives the same key.
fn password_bytes(password: &str) -> Zeroizing<Vec<u8>> {
    let normalised: Zeroizing<String> = Zeroizing::new(password.nfkc().collect());
    Zeroizing::new(normalised.as_bytes().to_vec())
}

/// The key a password makes with a salt and settings: the master key for an
/// account, and the key of an export file. The password is normalised first, so
/// the same text gives the same key on every platform.
pub fn password_key(
    password: &str,
    salt: &[u8; SALT_LEN],
    params: &KdfParams,
) -> Result<SecretKey> {
    master_key(password, salt, params)
}

fn master_key(password: &str, salt: &[u8; SALT_LEN], params: &KdfParams) -> Result<SecretKey> {
    kdf::derive_master_key(&password_bytes(password), salt, params)
}

/// Creates the keys for a new account. `user` is the account's ID, which
/// every wrapped key is bound to.
pub fn create(user: Id, password: &str, params: KdfParams) -> Result<NewAccount> {
    let salt = kdf::generate_salt()?;
    let master = master_key(password, &salt, &params)?;
    let recovery = RecoveryPhrase::generate()?;
    let user_key = SecretKey::generate()?;
    let (private, public) = seal::generate_keypair()?;

    let bundle = AccountBundle {
        version: BUNDLE_VERSION,
        kdf_salt: salt,
        kdf_params: params,
        user_key_by_master: wrap::wrap_key(
            &master,
            &context(Purpose::UserKeyByMaster, user),
            &user_key,
        )?,
        user_key_by_recovery: wrap::wrap_key(
            &recovery.recovery_key()?,
            &context(Purpose::UserKeyByRecovery, user),
            &user_key,
        )?,
        public_x25519: *public.as_bytes(),
        private_keys_enc: crate::aead::encrypt(
            &user_key,
            &context(Purpose::PrivateKeys, user),
            private.expose(),
        )?,
    };
    Ok(NewAccount {
        bundle,
        user_key,
        recovery,
    })
}

fn check_version(bundle: &AccountBundle) -> Result<()> {
    if bundle.version == BUNDLE_VERSION {
        Ok(())
    } else {
        Err(Error::UnsupportedVersion(bundle.version))
    }
}

/// Unwraps the user key with the master password. A wrong password and a
/// tampered bundle both give [`Error::Decrypt`]. Stored parameters outside the
/// allowed range give [`Error::KdfParams`] before any work is done.
pub fn unlock_with_password(user: Id, bundle: &AccountBundle, password: &str) -> Result<SecretKey> {
    check_version(bundle)?;
    let master = master_key(password, &bundle.kdf_salt, &bundle.kdf_params)?;
    wrap::unwrap_key(
        &master,
        &context(Purpose::UserKeyByMaster, user),
        &bundle.user_key_by_master,
    )
}

/// Unwraps the user key with the recovery phrase.
pub fn unlock_with_recovery(
    user: Id,
    bundle: &AccountBundle,
    phrase: &RecoveryPhrase,
) -> Result<SecretKey> {
    check_version(bundle)?;
    wrap::unwrap_key(
        &phrase.recovery_key()?,
        &context(Purpose::UserKeyByRecovery, user),
        &bundle.user_key_by_recovery,
    )
}

/// Wraps the existing user key under a new master password, with a fresh
/// salt. Used for changing the password and after recovering with the phrase.
pub fn rewrap_master(
    user: Id,
    user_key: &SecretKey,
    new_password: &str,
    params: KdfParams,
) -> Result<MasterRewrap> {
    let salt = kdf::generate_salt()?;
    let master = master_key(new_password, &salt, &params)?;
    Ok(MasterRewrap {
        kdf_salt: salt,
        kdf_params: params,
        user_key_by_master: wrap::wrap_key(
            &master,
            &context(Purpose::UserKeyByMaster, user),
            user_key,
        )?,
    })
}

/// Decrypts the X25519 private key, for opening sealed vault keys.
pub fn open_private_key(
    user: Id,
    user_key: &SecretKey,
    bundle: &AccountBundle,
) -> Result<PrivateKey> {
    check_version(bundle)?;
    let bytes = crate::aead::decrypt(
        user_key,
        &context(Purpose::PrivateKeys, user),
        &bundle.private_keys_enc,
    )?;
    if bytes.len() != PRIVATE_KEY_LEN {
        return Err(Error::Format);
    }
    let private = PrivateKey::from_slice(&bytes)?;
    // The public half the server holds must belong to this private key.
    if private.public_key()?.as_bytes() != &bundle.public_x25519 {
        return Err(Error::Format);
    }
    Ok(private)
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: Id = [7; 16];
    const FAST: KdfParams = KdfParams::MIN;

    #[test]
    fn password_unlocks_and_wrong_password_does_not() {
        let account = create(USER, "correct horse", FAST).unwrap();
        let key = unlock_with_password(USER, &account.bundle, "correct horse").unwrap();
        assert_eq!(key.expose(), account.user_key.expose());
        assert_eq!(
            unlock_with_password(USER, &account.bundle, "correct horsf").unwrap_err(),
            Error::Decrypt
        );
    }

    #[test]
    fn the_recovery_phrase_unlocks_the_same_key() {
        let account = create(USER, "correct horse", FAST).unwrap();
        let key = unlock_with_recovery(USER, &account.bundle, &account.recovery).unwrap();
        assert_eq!(key.expose(), account.user_key.expose());

        let other = RecoveryPhrase::generate().unwrap();
        assert_eq!(
            unlock_with_recovery(USER, &account.bundle, &other).unwrap_err(),
            Error::Decrypt
        );
    }

    #[test]
    fn a_bundle_does_not_unlock_for_another_user() {
        let account = create(USER, "correct horse", FAST).unwrap();
        assert_eq!(
            unlock_with_password([8; 16], &account.bundle, "correct horse").unwrap_err(),
            Error::Decrypt
        );
    }

    #[test]
    fn equivalent_unicode_passwords_give_the_same_key() {
        // "é" as one code point, as e plus a combining accent, and a
        // fullwidth "ｅ" all normalise to the same NFKC form where applicable.
        let account = create(USER, "caf\u{e9} 1", FAST).unwrap();
        assert!(unlock_with_password(USER, &account.bundle, "cafe\u{301} 1").is_ok());
        let fullwidth = create(USER, "\u{ff21}bc", FAST).unwrap();
        assert!(unlock_with_password(USER, &fullwidth.bundle, "Abc").is_ok());
    }

    #[test]
    fn changing_the_password_rewraps_without_changing_the_user_key() {
        let account = create(USER, "old password", FAST).unwrap();
        let rewrap = rewrap_master(USER, &account.user_key, "new password", FAST).unwrap();
        assert_ne!(rewrap.kdf_salt, account.bundle.kdf_salt);

        let updated = AccountBundle {
            kdf_salt: rewrap.kdf_salt,
            kdf_params: rewrap.kdf_params,
            user_key_by_master: rewrap.user_key_by_master,
            ..account.bundle
        };
        let key = unlock_with_password(USER, &updated, "new password").unwrap();
        assert_eq!(key.expose(), account.user_key.expose());
        assert!(unlock_with_password(USER, &updated, "old password").is_err());
        // The recovery phrase still works after a password change.
        assert!(unlock_with_recovery(USER, &updated, &account.recovery).is_ok());
    }

    #[test]
    fn the_private_key_matches_the_published_public_key() {
        let account = create(USER, "correct horse", FAST).unwrap();
        let private = open_private_key(USER, &account.user_key, &account.bundle).unwrap();
        assert_eq!(
            private.public_key().unwrap().as_bytes(),
            &account.bundle.public_x25519
        );

        let mut swapped = account.bundle.clone();
        swapped.public_x25519 = [9; PUBLIC_KEY_LEN];
        assert_eq!(
            open_private_key(USER, &account.user_key, &swapped).unwrap_err(),
            Error::Format
        );
    }

    #[test]
    fn weak_stored_parameters_are_refused_before_deriving() {
        let mut account = create(USER, "correct horse", FAST).unwrap();
        account.bundle.kdf_params.iterations = 1;
        assert_eq!(
            unlock_with_password(USER, &account.bundle, "correct horse").unwrap_err(),
            Error::KdfParams
        );
    }

    #[test]
    fn an_unknown_bundle_version_is_refused() {
        let mut account = create(USER, "correct horse", FAST).unwrap();
        account.bundle.version = 2;
        assert_eq!(
            unlock_with_password(USER, &account.bundle, "correct horse").unwrap_err(),
            Error::UnsupportedVersion(2)
        );
    }
}
