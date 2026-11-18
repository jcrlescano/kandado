//! Argon2id: turns the master password into the master key.
//!
//! The salt and parameters are stored per user, so they can be raised later
//! by re-wrapping the user key.

use argon2::{Algorithm, Argon2, Params, Version};

use crate::{Error, KEY_LEN, Result, SecretKey, random};

pub const SALT_LEN: usize = 16;

/// Argon2id cost settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory in kibibytes.
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl KdfParams {
    /// What new accounts get: 64 MiB, 3 passes, 1 lane. One lane because the
    /// browser extension derives the same key in single-threaded WebAssembly.
    pub const DEFAULT: Self = Self {
        memory_kib: 64 * 1024,
        iterations: 3,
        parallelism: 1,
    };

    /// The weakest settings a client accepts (the OWASP minimum for Argon2id).
    /// A server that hands out weaker ones is refused, not obeyed.
    pub const MIN: Self = Self {
        memory_kib: 19 * 1024,
        iterations: 2,
        parallelism: 1,
    };

    /// The most a client is willing to spend, so stored parameters cannot be
    /// used to exhaust memory or hang the device.
    pub const MAX: Self = Self {
        memory_kib: 1024 * 1024,
        iterations: 16,
        parallelism: 8,
    };

    pub fn validate(&self) -> Result<()> {
        let in_range = (Self::MIN.memory_kib..=Self::MAX.memory_kib).contains(&self.memory_kib)
            && (Self::MIN.iterations..=Self::MAX.iterations).contains(&self.iterations)
            && (Self::MIN.parallelism..=Self::MAX.parallelism).contains(&self.parallelism);
        if in_range {
            Ok(())
        } else {
            Err(Error::KdfParams)
        }
    }
}

/// Creates a random salt for a new account or a password change.
pub fn generate_salt() -> Result<[u8; SALT_LEN]> {
    let mut salt = [0; SALT_LEN];
    random::fill(&mut salt)?;
    Ok(salt)
}

/// Derives the master key from the master password.
///
/// The password is used as the exact bytes given. Callers normalise it to
/// Unicode NFKC first, so the same password typed on different platforms
/// gives the same key.
pub fn derive_master_key(
    password: &[u8],
    salt: &[u8; SALT_LEN],
    params: &KdfParams,
) -> Result<SecretKey> {
    params.validate()?;
    let argon_params = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        Some(KEY_LEN),
    )
    .map_err(|_| Error::KdfParams)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut out = zeroize::Zeroizing::new([0; KEY_LEN]);
    argon
        .hash_password_into(password, salt, out.as_mut_slice())
        .map_err(|_| Error::KdfParams)?;
    Ok(SecretKey::from_bytes(*out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_inputs_give_the_same_key() {
        let salt = [3; SALT_LEN];
        let a = derive_master_key(b"correct horse", &salt, &KdfParams::MIN).unwrap();
        let b = derive_master_key(b"correct horse", &salt, &KdfParams::MIN).unwrap();
        assert_eq!(a.expose(), b.expose());
    }

    #[test]
    fn password_salt_and_params_all_change_the_key() {
        let base = derive_master_key(b"correct horse", &[3; SALT_LEN], &KdfParams::MIN).unwrap();
        let password =
            derive_master_key(b"correct horsf", &[3; SALT_LEN], &KdfParams::MIN).unwrap();
        let salt = derive_master_key(b"correct horse", &[4; SALT_LEN], &KdfParams::MIN).unwrap();
        let harder = KdfParams {
            iterations: 3,
            ..KdfParams::MIN
        };
        let params = derive_master_key(b"correct horse", &[3; SALT_LEN], &harder).unwrap();
        assert_ne!(base.expose(), password.expose());
        assert_ne!(base.expose(), salt.expose());
        assert_ne!(base.expose(), params.expose());
    }

    #[test]
    fn refuses_parameters_outside_the_allowed_range() {
        let salt = [0; SALT_LEN];
        let out_of_range = [
            KdfParams {
                memory_kib: KdfParams::MIN.memory_kib - 1,
                ..KdfParams::MIN
            },
            KdfParams {
                iterations: 1,
                ..KdfParams::MIN
            },
            KdfParams {
                parallelism: 0,
                ..KdfParams::MIN
            },
            KdfParams {
                memory_kib: KdfParams::MAX.memory_kib + 1,
                ..KdfParams::MAX
            },
            KdfParams {
                iterations: KdfParams::MAX.iterations + 1,
                ..KdfParams::MAX
            },
            KdfParams {
                parallelism: KdfParams::MAX.parallelism + 1,
                ..KdfParams::MAX
            },
        ];
        for params in out_of_range {
            assert_eq!(
                derive_master_key(b"pw", &salt, &params).unwrap_err(),
                Error::KdfParams
            );
        }
        assert!(KdfParams::DEFAULT.validate().is_ok());
    }
}
