//! The device key: lets Windows Hello unlock without the master password.
//!
//! The user key is wrapped by a key only this computer can produce. On
//! Windows that key comes from a Windows Hello credential (the signature over
//! a fixed challenge is stable and secret to the TPM-backed key), or from
//! DPAPI when Hello is not set up. The wrapped copy lives in the local store
//! and is deleted on sign-out.

use kandado_crypto::{Context, Purpose, SecretKey, wrap};
use kandado_vault::store::DeviceWrap;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum DeviceKeyError {
    #[error("Windows Hello is not available on this computer")]
    Unavailable,
    #[error("Windows Hello was cancelled or did not recognise you")]
    Declined,
    #[error("Windows Hello failed: {0}")]
    Failed(String),
}

/// Something that can protect and later recover the user key on this device.
pub trait DeviceKey: Send + Sync {
    /// True when [`DeviceKey::protect`] can work here.
    fn available(&self) -> bool;
    /// Wraps the user key for this device. May ask the person to verify.
    fn protect(
        &self,
        user: Uuid,
        device: Uuid,
        user_key: &SecretKey,
    ) -> Result<DeviceWrap, DeviceKeyError>;
    /// Recovers the user key. Asks the person to verify.
    fn recover(
        &self,
        user: Uuid,
        device: Uuid,
        wrap: &DeviceWrap,
    ) -> Result<SecretKey, DeviceKeyError>;
    /// Removes whatever the key needs outside the store.
    fn forget(&self);
}

pub(crate) fn context(user: Uuid, device: Uuid) -> Context {
    Context::new(
        Purpose::UserKeyByDevice,
        *user.as_bytes(),
        *device.as_bytes(),
        1,
    )
}

/// Wraps and unwraps with a key supplied by the caller. Shared by every
/// implementation, so the format is the same whatever produced the key.
pub(crate) fn wrap_with(
    device_key: &SecretKey,
    user: Uuid,
    device: Uuid,
    user_key: &SecretKey,
) -> Result<Vec<u8>, DeviceKeyError> {
    wrap::wrap_key(device_key, &context(user, device), user_key)
        .map_err(|error| DeviceKeyError::Failed(error.to_string()))
}

pub(crate) fn unwrap_with(
    device_key: &SecretKey,
    user: Uuid,
    device: Uuid,
    wrapped: &[u8],
) -> Result<SecretKey, DeviceKeyError> {
    wrap::unwrap_key(device_key, &context(user, device), wrapped)
        .map_err(|error| DeviceKeyError::Failed(error.to_string()))
}

/// For tests and for platforms without a device key: remembers a random key
/// in memory only, so a wrap made in one run cannot be opened in the next.
pub struct MemoryDeviceKey {
    key: SecretKey,
    pub available: bool,
}

impl MemoryDeviceKey {
    pub fn new() -> Self {
        Self {
            key: SecretKey::generate().expect("system random works"),
            available: true,
        }
    }
}

impl Default for MemoryDeviceKey {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceKey for MemoryDeviceKey {
    fn available(&self) -> bool {
        self.available
    }

    fn protect(
        &self,
        user: Uuid,
        device: Uuid,
        user_key: &SecretKey,
    ) -> Result<DeviceWrap, DeviceKeyError> {
        if !self.available {
            return Err(DeviceKeyError::Unavailable);
        }
        Ok(DeviceWrap {
            method: "memory".into(),
            wrapped: wrap_with(&self.key, user, device, user_key)?,
        })
    }

    fn recover(
        &self,
        user: Uuid,
        device: Uuid,
        wrap: &DeviceWrap,
    ) -> Result<SecretKey, DeviceKeyError> {
        unwrap_with(&self.key, user, device, &wrap.wrapped)
    }

    fn forget(&self) {}
}

#[cfg(windows)]
pub mod windows_hello {
    #![allow(unsafe_code)]

    use kandado_crypto::SecretKey;
    use sha2::{Digest, Sha256};
    use windows::{
        Security::{
            Credentials::{KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus},
            Cryptography::CryptographicBuffer,
        },
        Win32::{
            Foundation::{HLOCAL, LocalFree},
            Security::Cryptography::{
                CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
            },
        },
        core::HSTRING,
    };

    use super::{DeviceKey, DeviceKeyError, DeviceWrap, Uuid, unwrap_with, wrap_with};

    /// Name of the credential in Windows Hello.
    const CREDENTIAL_NAME: &str = crate::profile::NAME;
    /// What Hello signs. Any fixed value works: RSA PKCS#1 signatures are
    /// deterministic, so the same challenge gives the same bytes every time.
    const CHALLENGE: &[u8] = b"kandado/device-key/v1";
    const DOMAIN: &[u8] = b"kandado/device-key/v1/derive";

    fn failed(error: windows::core::Error) -> DeviceKeyError {
        DeviceKeyError::Failed(error.message())
    }

    /// Windows Hello when set up; DPAPI otherwise.
    pub struct WindowsDeviceKey;

    impl WindowsDeviceKey {
        pub fn new() -> Self {
            Self
        }

        fn hello_supported() -> bool {
            KeyCredentialManager::IsSupportedAsync()
                .and_then(|operation| operation.join())
                .unwrap_or(false)
        }

        /// Signs the fixed challenge with the Hello credential and turns the
        /// signature into a key.
        fn hello_key(create: bool) -> Result<SecretKey, DeviceKeyError> {
            let name = HSTRING::from(CREDENTIAL_NAME);
            let result = if create {
                KeyCredentialManager::RequestCreateAsync(
                    &name,
                    KeyCredentialCreationOption::ReplaceExisting,
                )
            } else {
                KeyCredentialManager::OpenAsync(&name)
            }
            .and_then(|operation| operation.join())
            .map_err(failed)?;
            match result.Status().map_err(failed)? {
                KeyCredentialStatus::Success => {}
                KeyCredentialStatus::UserCanceled | KeyCredentialStatus::UserPrefersPassword => {
                    return Err(DeviceKeyError::Declined);
                }
                KeyCredentialStatus::NotFound => return Err(DeviceKeyError::Unavailable),
                other => return Err(DeviceKeyError::Failed(format!("{other:?}"))),
            }
            let credential = result.Credential().map_err(failed)?;
            let challenge = CryptographicBuffer::CreateFromByteArray(CHALLENGE).map_err(failed)?;
            let signed = credential
                .RequestSignAsync(&challenge)
                .and_then(|operation| operation.join())
                .map_err(failed)?;
            match signed.Status().map_err(failed)? {
                KeyCredentialStatus::Success => {}
                KeyCredentialStatus::UserCanceled | KeyCredentialStatus::UserPrefersPassword => {
                    return Err(DeviceKeyError::Declined);
                }
                other => return Err(DeviceKeyError::Failed(format!("{other:?}"))),
            }
            let buffer = signed.Result().map_err(failed)?;
            let mut signature = windows::core::Array::<u8>::new();
            CryptographicBuffer::CopyToByteArray(&buffer, &mut signature).map_err(failed)?;
            let mut hasher = Sha256::new();
            hasher.update(DOMAIN);
            hasher.update(signature.as_slice());
            Ok(SecretKey::from_bytes(hasher.finalize().into()))
        }

        fn dpapi_protect(secret: &[u8]) -> Result<Vec<u8>, DeviceKeyError> {
            let input = CRYPT_INTEGER_BLOB {
                cbData: secret.len() as u32,
                pbData: secret.as_ptr() as *mut u8,
            };
            let mut output = CRYPT_INTEGER_BLOB::default();
            // SAFETY: `input` points at `secret` for the call; on success
            // `output` holds memory this function frees with `LocalFree`.
            unsafe {
                CryptProtectData(
                    &input,
                    None,
                    None,
                    None,
                    None,
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
                .map_err(failed)?;
                let bytes =
                    std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
                let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
                Ok(bytes)
            }
        }

        fn dpapi_unprotect(blob: &[u8]) -> Result<Vec<u8>, DeviceKeyError> {
            let input = CRYPT_INTEGER_BLOB {
                cbData: blob.len() as u32,
                pbData: blob.as_ptr() as *mut u8,
            };
            let mut output = CRYPT_INTEGER_BLOB::default();
            // SAFETY: as in `dpapi_protect`.
            unsafe {
                CryptUnprotectData(
                    &input,
                    None,
                    None,
                    None,
                    None,
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
                .map_err(failed)?;
                let bytes =
                    std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
                std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize).fill(0);
                let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
                Ok(bytes)
            }
        }
    }

    impl Default for WindowsDeviceKey {
        fn default() -> Self {
            Self::new()
        }
    }

    impl DeviceKey for WindowsDeviceKey {
        fn available(&self) -> bool {
            true
        }

        fn protect(
            &self,
            user: Uuid,
            device: Uuid,
            user_key: &SecretKey,
        ) -> Result<DeviceWrap, DeviceKeyError> {
            if Self::hello_supported() {
                match Self::hello_key(true) {
                    Ok(key) => {
                        return Ok(DeviceWrap {
                            method: "hello".into(),
                            wrapped: wrap_with(&key, user, device, user_key)?,
                        });
                    }
                    Err(DeviceKeyError::Declined) => return Err(DeviceKeyError::Declined),
                    Err(_) => {}
                }
            }
            // No Hello: a random key kept under DPAPI, which ties it to this
            // Windows account on this computer.
            let key = SecretKey::generate().map_err(|e| DeviceKeyError::Failed(e.to_string()))?;
            let protected = Self::dpapi_protect(key.expose())?;
            let mut wrapped = (protected.len() as u32).to_le_bytes().to_vec();
            wrapped.extend_from_slice(&protected);
            wrapped.extend_from_slice(&wrap_with(&key, user, device, user_key)?);
            Ok(DeviceWrap {
                method: "dpapi".into(),
                wrapped,
            })
        }

        fn recover(
            &self,
            user: Uuid,
            device: Uuid,
            wrap: &DeviceWrap,
        ) -> Result<SecretKey, DeviceKeyError> {
            match wrap.method.as_str() {
                "hello" => unwrap_with(&Self::hello_key(false)?, user, device, &wrap.wrapped),
                "dpapi" => {
                    let (length, rest) = wrap
                        .wrapped
                        .split_first_chunk::<4>()
                        .ok_or_else(|| DeviceKeyError::Failed("malformed".into()))?;
                    let length = u32::from_le_bytes(*length) as usize;
                    if rest.len() < length {
                        return Err(DeviceKeyError::Failed("malformed".into()));
                    }
                    let (protected, wrapped) = rest.split_at(length);
                    let key = SecretKey::from_slice(&Self::dpapi_unprotect(protected)?)
                        .map_err(|e| DeviceKeyError::Failed(e.to_string()))?;
                    unwrap_with(&key, user, device, wrapped)
                }
                other => Err(DeviceKeyError::Failed(format!("unknown method {other}"))),
            }
        }

        fn forget(&self) {
            // Removing the credential makes any Hello-wrapped copy useless.
            let _ = KeyCredentialManager::DeleteAsync(&HSTRING::from(CREDENTIAL_NAME))
                .and_then(|operation| operation.join());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrap_recovers_the_same_key_for_the_same_user_and_device() {
        let key = MemoryDeviceKey::new();
        let user_key = SecretKey::generate().unwrap();
        let (user, device) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let wrap = key.protect(user, device, &user_key).unwrap();
        let back = key.recover(user, device, &wrap).unwrap();
        assert_eq!(back.expose(), user_key.expose());
        // Bound to the user and device: a copied file does not open elsewhere.
        assert!(key.recover(Uuid::from_u128(9), device, &wrap).is_err());
        assert!(key.recover(user, Uuid::from_u128(9), &wrap).is_err());
    }

    #[test]
    fn an_unavailable_key_refuses_to_protect() {
        let mut key = MemoryDeviceKey::new();
        key.available = false;
        assert!(!key.available());
        assert!(matches!(
            key.protect(Uuid::nil(), Uuid::nil(), &SecretKey::generate().unwrap()),
            Err(DeviceKeyError::Unavailable)
        ));
    }
}
