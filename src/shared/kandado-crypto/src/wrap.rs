//! Key wrapping: one symmetric key encrypted under another.
//!
//! Used for the user key (under the master, recovery and device keys) and
//! for item keys (under their vault key). A wrapped key is an [`aead`] blob
//! whose plaintext is exactly one key.
//!
//! [`aead`]: crate::aead

use crate::{Context, Error, KEY_LEN, Result, SecretKey, aead};

/// Length of a wrapped key in bytes.
pub const WRAPPED_LEN: usize = KEY_LEN + aead::OVERHEAD;

/// Encrypts `key` under `wrapping_key`, bound to `context`.
pub fn wrap_key(wrapping_key: &SecretKey, context: &Context, key: &SecretKey) -> Result<Vec<u8>> {
    aead::encrypt(wrapping_key, context, key.expose())
}

/// Recovers a key wrapped by [`wrap_key`] with the same wrapping key and context.
pub fn unwrap_key(
    wrapping_key: &SecretKey,
    context: &Context,
    wrapped: &[u8],
) -> Result<SecretKey> {
    if wrapped.len() != WRAPPED_LEN {
        return Err(Error::Format);
    }
    let bytes = aead::decrypt(wrapping_key, context, wrapped)?;
    SecretKey::from_slice(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Purpose, context::NO_ID};

    fn context() -> Context {
        Context::new(Purpose::UserKeyByMaster, [1; 16], NO_ID, 1)
    }

    #[test]
    fn round_trips() {
        let wrapping = SecretKey::generate().unwrap();
        let key = SecretKey::generate().unwrap();
        let wrapped = wrap_key(&wrapping, &context(), &key).unwrap();
        assert_eq!(wrapped.len(), WRAPPED_LEN);
        assert_eq!(
            unwrap_key(&wrapping, &context(), &wrapped)
                .unwrap()
                .expose(),
            key.expose()
        );
    }

    #[test]
    fn a_key_wrapped_for_one_purpose_does_not_unwrap_for_another() {
        let wrapping = SecretKey::generate().unwrap();
        let wrapped = wrap_key(&wrapping, &context(), &SecretKey::generate().unwrap()).unwrap();
        let other = Context {
            purpose: Purpose::UserKeyByRecovery,
            ..context()
        };
        assert_eq!(
            unwrap_key(&wrapping, &other, &wrapped).unwrap_err(),
            Error::Decrypt
        );
    }

    #[test]
    fn rejects_the_wrong_wrapping_key() {
        let wrapped = wrap_key(
            &SecretKey::generate().unwrap(),
            &context(),
            &SecretKey::generate().unwrap(),
        )
        .unwrap();
        let other = SecretKey::generate().unwrap();
        assert_eq!(
            unwrap_key(&other, &context(), &wrapped).unwrap_err(),
            Error::Decrypt
        );
    }

    #[test]
    fn rejects_blobs_that_do_not_hold_exactly_one_key() {
        let wrapping = SecretKey::generate().unwrap();
        let not_a_key = aead::encrypt(&wrapping, &context(), &[0; KEY_LEN + 1]).unwrap();
        assert_eq!(
            unwrap_key(&wrapping, &context(), &not_a_key).unwrap_err(),
            Error::Format
        );
    }
}
