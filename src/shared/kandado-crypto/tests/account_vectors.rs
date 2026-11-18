//! Known-answer tests against `vectors/account-v1.json`: a complete account
//! bundle that must keep unlocking with the same password and recovery phrase.
//! The file is frozen. A change that makes these fail has broken compatibility
//! with every stored account.
//!
//! To add vectors for a new bundle version, extend `write_vectors` to write a
//! new file and run
//! `cargo test -p kandado-crypto --test account_vectors -- --ignored write_vectors`.

use kandado_crypto::{
    account::{self, AccountBundle},
    kdf::KdfParams,
    recovery::RecoveryPhrase,
};
use serde::{Deserialize, Serialize};

const VECTORS: &str = include_str!("../vectors/account-v1.json");

#[derive(Serialize, Deserialize)]
struct Vector {
    user: String,
    password: String,
    phrase: String,
    user_key: String,
    version: u8,
    kdf_salt: String,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    user_key_by_master: String,
    user_key_by_recovery: String,
    public_x25519: String,
    private_keys_enc: String,
}

fn array<const N: usize>(hex_text: &str) -> [u8; N] {
    hex::decode(hex_text).unwrap().try_into().unwrap()
}

impl Vector {
    fn bundle(&self) -> AccountBundle {
        AccountBundle {
            version: self.version,
            kdf_salt: array(&self.kdf_salt),
            kdf_params: KdfParams {
                memory_kib: self.memory_kib,
                iterations: self.iterations,
                parallelism: self.parallelism,
            },
            user_key_by_master: hex::decode(&self.user_key_by_master).unwrap(),
            user_key_by_recovery: hex::decode(&self.user_key_by_recovery).unwrap(),
            public_x25519: array(&self.public_x25519),
            private_keys_enc: hex::decode(&self.private_keys_enc).unwrap(),
        }
    }
}

#[test]
fn stored_bundles_still_unlock() {
    let vectors: Vec<Vector> = serde_json::from_str(VECTORS).unwrap();
    assert!(!vectors.is_empty());
    for vector in vectors {
        let user = array(&vector.user);
        let bundle = vector.bundle();

        let by_password = account::unlock_with_password(user, &bundle, &vector.password).unwrap();
        assert_eq!(hex::encode(by_password.expose()), vector.user_key);

        let phrase = RecoveryPhrase::parse(&vector.phrase).unwrap();
        let by_phrase = account::unlock_with_recovery(user, &bundle, &phrase).unwrap();
        assert_eq!(hex::encode(by_phrase.expose()), vector.user_key);

        let private = account::open_private_key(user, &by_password, &bundle).unwrap();
        assert_eq!(
            private.public_key().unwrap().as_bytes(),
            &bundle.public_x25519
        );
    }
}

#[test]
#[ignore = "writes vectors/account-v1.json"]
fn write_vectors() {
    let user = [0x42; 16];
    let password = "correct horse battery staple";
    let params = KdfParams::MIN;
    let created = account::create(user, password, params).unwrap();
    let phrase = created.recovery.words().collect::<Vec<_>>().join(" ");
    let bundle = &created.bundle;
    let vector = Vector {
        user: hex::encode(user),
        password: password.into(),
        phrase,
        user_key: hex::encode(created.user_key.expose()),
        version: bundle.version,
        kdf_salt: hex::encode(bundle.kdf_salt),
        memory_kib: params.memory_kib,
        iterations: params.iterations,
        parallelism: params.parallelism,
        user_key_by_master: hex::encode(&bundle.user_key_by_master),
        user_key_by_recovery: hex::encode(&bundle.user_key_by_recovery),
        public_x25519: hex::encode(bundle.public_x25519),
        private_keys_enc: hex::encode(&bundle.private_keys_enc),
    };
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/vectors/account-v1.json");
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        existing.trim().is_empty() || existing.trim() == "[]",
        "account-v1.json is frozen"
    );
    std::fs::write(
        path,
        serde_json::to_string_pretty(&[vector]).unwrap() + "\n",
    )
    .unwrap();
}
