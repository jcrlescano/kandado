//! The account and its wrapped keys.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Opaque;

/// `GET /v1/me`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Me {
    pub id: Uuid,
    /// From Google. Shown read-only at D-02.
    pub email: String,
    pub display_name: String,
    pub plan: String,
    /// False until the first device uploads keys (D-02).
    pub has_keys: bool,
    /// The calling device.
    pub device_id: Uuid,
}

/// Argon2id settings, stored with the keys they produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfSettings {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

/// The keys the server holds for a user. Everything is ciphertext or public.
///
/// `POST /v1/account/keys` stores them once; `GET /v1/account/keys` returns
/// them to a new device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountKeys {
    pub version: u8,
    pub kdf_salt: Opaque,
    pub kdf: KdfSettings,
    pub user_key_by_master: Opaque,
    pub user_key_by_recovery: Opaque,
    pub public_x25519: Opaque,
    pub private_keys_enc: Opaque,
}

/// `PUT /v1/account/keys/master`: replaces the master-password wrapping.
/// Needs a fresh Google sign-in (step-up).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeMaster {
    pub kdf_salt: Opaque,
    pub kdf: KdfSettings,
    pub user_key_by_master: Opaque,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_wire_format() {
        let keys = AccountKeys {
            version: 1,
            kdf_salt: Opaque::new(vec![1, 2]),
            kdf: KdfSettings {
                memory_kib: 65536,
                iterations: 3,
                parallelism: 1,
            },
            user_key_by_master: Opaque::new(vec![3]),
            user_key_by_recovery: Opaque::new(vec![4]),
            public_x25519: Opaque::new(vec![5]),
            private_keys_enc: Opaque::new(vec![6]),
        };
        let json = serde_json::to_string(&keys).unwrap();
        assert_eq!(
            json,
            r#"{"version":1,"kdf_salt":"AQI=","kdf":{"memory_kib":65536,"iterations":3,"parallelism":1},"user_key_by_master":"Aw==","user_key_by_recovery":"BA==","public_x25519":"BQ==","private_keys_enc":"Bg=="}"#
        );
        assert_eq!(serde_json::from_str::<AccountKeys>(&json).unwrap(), keys);
    }
}
