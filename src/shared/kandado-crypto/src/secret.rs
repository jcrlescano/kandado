use core::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::{Error, Result, random};

/// Length of every symmetric key in bytes.
pub const KEY_LEN: usize = 32;

/// A 256-bit symmetric key.
///
/// Wiped from memory when dropped. It cannot be printed, compared or
/// serialised; the only way to the bytes is [`SecretKey::expose`].
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretKey([u8; KEY_LEN]);

impl SecretKey {
    /// Creates a new random key.
    pub fn generate() -> Result<Self> {
        let mut key = Self([0; KEY_LEN]);
        random::fill(&mut key.0)?;
        Ok(key)
    }

    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }

    /// Copies a key out of a slice, which must be exactly [`KEY_LEN`] bytes.
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let mut key = Self([0; KEY_LEN]);
        if bytes.len() != KEY_LEN {
            return Err(Error::InvalidKey);
        }
        key.0.copy_from_slice(bytes);
        Ok(key)
    }

    /// The raw key bytes. Keep the borrow short and never log or store it.
    pub fn expose(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey(..)")
    }
}

/// Decrypted bytes, wiped from memory when dropped.
pub type SecretBytes = Zeroizing<Vec<u8>>;
