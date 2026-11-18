//! Small secrets the app must keep between runs: the refresh token. On
//! Windows they live in Credential Manager, protected by the person's Windows
//! sign-in; nothing is written to `settings.json` or the vault file.

use std::{collections::HashMap, sync::Mutex};

#[derive(Debug, thiserror::Error)]
pub enum SecretsError {
    #[error("the secret store failed: {0}")]
    Failed(String),
}

pub trait SecretStore: Send + Sync {
    fn save(&self, name: &str, secret: &[u8]) -> Result<(), SecretsError>;
    fn load(&self, name: &str) -> Result<Option<Vec<u8>>, SecretsError>;
    fn delete(&self, name: &str) -> Result<(), SecretsError>;
}

/// Holds secrets in memory only. For tests, and for platforms without a
/// native store yet.
#[derive(Default)]
pub struct MemorySecrets {
    items: Mutex<HashMap<String, Vec<u8>>>,
}

impl SecretStore for MemorySecrets {
    fn save(&self, name: &str, secret: &[u8]) -> Result<(), SecretsError> {
        self.items
            .lock()
            .expect("secrets lock")
            .insert(name.to_owned(), secret.to_vec());
        Ok(())
    }

    fn load(&self, name: &str) -> Result<Option<Vec<u8>>, SecretsError> {
        Ok(self.items.lock().expect("secrets lock").get(name).cloned())
    }

    fn delete(&self, name: &str) -> Result<(), SecretsError> {
        self.items.lock().expect("secrets lock").remove(name);
        Ok(())
    }
}

/// Windows Credential Manager.
#[cfg(windows)]
pub mod windows_store {
    #![allow(unsafe_code)]

    use windows::{
        Win32::{
            Foundation::ERROR_NOT_FOUND,
            Security::Credentials::{
                CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW,
                CredDeleteW, CredFree, CredReadW, CredWriteW,
            },
        },
        core::{HSTRING, PWSTR},
    };

    use super::{SecretStore, SecretsError};

    /// Names are prefixed so Kandado's entries are easy to find in
    /// Credential Manager.
    pub struct CredentialManager {
        prefix: String,
    }

    impl CredentialManager {
        pub fn new(prefix: &str) -> Self {
            Self {
                prefix: prefix.to_owned(),
            }
        }

        fn target(&self, name: &str) -> HSTRING {
            HSTRING::from(format!("{}/{name}", self.prefix))
        }
    }

    fn failed(error: windows::core::Error) -> SecretsError {
        SecretsError::Failed(error.message())
    }

    impl SecretStore for CredentialManager {
        fn save(&self, name: &str, secret: &[u8]) -> Result<(), SecretsError> {
            let target = self.target(name);
            let mut blob = secret.to_vec();
            let credential = CREDENTIALW {
                Flags: CRED_FLAGS(0),
                Type: CRED_TYPE_GENERIC,
                TargetName: PWSTR(target.as_ptr() as *mut u16),
                CredentialBlobSize: blob.len() as u32,
                CredentialBlob: blob.as_mut_ptr(),
                Persist: CRED_PERSIST_LOCAL_MACHINE,
                ..Default::default()
            };
            // SAFETY: `credential` points at `target` and `blob`, which both
            // outlive the call; the API copies what it needs.
            let result = unsafe { CredWriteW(&credential, 0) };
            blob.fill(0);
            result.map_err(failed)
        }

        fn load(&self, name: &str) -> Result<Option<Vec<u8>>, SecretsError> {
            let target = self.target(name);
            let mut found: *mut CREDENTIALW = std::ptr::null_mut();
            // SAFETY: on success `found` points at a credential that is read
            // here and then released with `CredFree`.
            let result = unsafe { CredReadW(&target, CRED_TYPE_GENERIC, None, &mut found) };
            match result {
                Ok(()) => {
                    let secret = unsafe {
                        let credential = &*found;
                        let bytes = std::slice::from_raw_parts(
                            credential.CredentialBlob,
                            credential.CredentialBlobSize as usize,
                        )
                        .to_vec();
                        CredFree(found.cast());
                        bytes
                    };
                    Ok(Some(secret))
                }
                Err(error) if error.code() == ERROR_NOT_FOUND.to_hresult() => Ok(None),
                Err(error) => Err(failed(error)),
            }
        }

        fn delete(&self, name: &str) -> Result<(), SecretsError> {
            let target = self.target(name);
            // SAFETY: `target` is a valid, null-terminated wide string.
            match unsafe { CredDeleteW(&target, CRED_TYPE_GENERIC, None) } {
                Ok(()) => Ok(()),
                Err(error) if error.code() == ERROR_NOT_FOUND.to_hresult() => Ok(()),
                Err(error) => Err(failed(error)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trips() {
        let store = MemorySecrets::default();
        assert_eq!(store.load("a").unwrap(), None);
        store.save("a", b"one").unwrap();
        store.save("a", b"two").unwrap();
        assert_eq!(store.load("a").unwrap().as_deref(), Some(&b"two"[..]));
        store.delete("a").unwrap();
        store.delete("a").unwrap();
        assert_eq!(store.load("a").unwrap(), None);
    }

    #[cfg(windows)]
    #[test]
    fn credential_manager_round_trips() {
        let store = windows_store::CredentialManager::new("KandadoTest");
        let name = format!("secret-{}", std::process::id());
        assert_eq!(store.load(&name).unwrap(), None);
        store.save(&name, b"refresh-token-value").unwrap();
        assert_eq!(
            store.load(&name).unwrap().as_deref(),
            Some(&b"refresh-token-value"[..])
        );
        store.save(&name, b"replaced").unwrap();
        assert_eq!(
            store.load(&name).unwrap().as_deref(),
            Some(&b"replaced"[..])
        );
        store.delete(&name).unwrap();
        assert_eq!(store.load(&name).unwrap(), None);
    }
}
