//! The unlocked session, end to end, in the WebAssembly build.
//!
//! The data is made with the native crypto crates and the shared contexts, the
//! way the desktop app makes it, then read back through `Session`. This is the
//! check that the extension opens what the desktop writes (and the reverse).
//!
//! `cargo test -p kandado-wasm --target wasm32-unknown-unknown --test session`
//! (runs in Node, which has the Web Crypto random source the build needs).
#![cfg(target_arch = "wasm32")]

use kandado_crypto::{
    SecretKey, account, aead,
    kdf::KdfParams,
    seal::{self, PublicKey},
    wrap,
};
use kandado_model::{
    ItemContent, ItemData, VaultMeta, contexts,
    item::{Login, Website},
};
use kandado_protocol::{AccountKeys, KdfSettings, Opaque};
use kandado_wasm::Session;
use uuid::Uuid;
use wasm_bindgen_test::*;

const USER: Uuid = Uuid::from_u128(0x0101);
const VAULT: Uuid = Uuid::from_u128(0x0202);
const ITEM: Uuid = Uuid::from_u128(0x0303);

struct World {
    keys_json: String,
    user_key: SecretKey,
    key_sealed: Vec<u8>,
    meta_enc: Vec<u8>,
    key_wrapped: Vec<u8>,
    content_enc: Vec<u8>,
    vault_key: SecretKey,
}

fn content() -> ItemContent {
    ItemContent::new(
        "GitHub",
        ItemData::Login(Login {
            username: "juan".into(),
            password: "correct horse".into(),
            websites: vec![Website {
                url: "https://github.com/login".into(),
                ..Default::default()
            }],
            totp: Some("JBSWY3DPEHPK3PXP".into()),
            ..Default::default()
        }),
    )
}

fn world() -> World {
    let account = account::create(*USER.as_bytes(), "master password", KdfParams::MIN).unwrap();
    let bundle = &account.bundle;
    let keys = AccountKeys {
        version: bundle.version,
        kdf_salt: Opaque::new(bundle.kdf_salt.to_vec()),
        kdf: KdfSettings {
            memory_kib: bundle.kdf_params.memory_kib,
            iterations: bundle.kdf_params.iterations,
            parallelism: bundle.kdf_params.parallelism,
        },
        user_key_by_master: Opaque::new(bundle.user_key_by_master.clone()),
        user_key_by_recovery: Opaque::new(bundle.user_key_by_recovery.clone()),
        public_x25519: Opaque::new(bundle.public_x25519.to_vec()),
        private_keys_enc: Opaque::new(bundle.private_keys_enc.clone()),
    };

    let vault_key = SecretKey::generate().unwrap();
    let key_sealed = seal::seal(
        &PublicKey::from_bytes(bundle.public_x25519),
        &contexts::vault_key(*USER.as_bytes(), *VAULT.as_bytes()),
        vault_key.expose(),
    )
    .unwrap();
    let meta_enc = aead::encrypt(
        &vault_key,
        &contexts::vault_meta(*VAULT.as_bytes()),
        &VaultMeta::new("Personal", "brass", "vault")
            .encode()
            .unwrap(),
    )
    .unwrap();
    let item_key = SecretKey::generate().unwrap();
    let key_wrapped = wrap::wrap_key(
        &vault_key,
        &contexts::item_key(*VAULT.as_bytes(), *ITEM.as_bytes()),
        &item_key,
    )
    .unwrap();
    let content_enc = aead::encrypt(
        &item_key,
        &contexts::item_content(*ITEM.as_bytes()),
        &content().encode().unwrap(),
    )
    .unwrap();
    World {
        keys_json: serde_json::to_string(&keys).unwrap(),
        user_key: account.user_key,
        key_sealed,
        meta_enc,
        key_wrapped,
        content_enc,
        vault_key,
    }
}

fn unlocked(world: &World) -> Session {
    let mut session =
        Session::unlock_with_password(&USER.to_string(), &world.keys_json, "master password")
            .unwrap_or_else(|_| panic!("unlock"));
    session
        .open_vault(&VAULT.to_string(), &world.key_sealed)
        .unwrap_or_else(|_| panic!("open vault"));
    session
}

#[wasm_bindgen_test]
fn the_extension_opens_what_the_desktop_writes() {
    let world = world();
    let session = unlocked(&world);
    let (vault, item) = (VAULT.to_string(), ITEM.to_string());

    let meta = session
        .vault_meta(&vault, &world.meta_enc)
        .unwrap_or_else(|_| panic!("meta"));
    assert!(meta.contains("\"Personal\""));

    let summary = session
        .item_summary(&vault, &item, &world.key_wrapped, &world.content_enc)
        .unwrap_or_else(|_| panic!("summary"));
    assert!(summary.contains("\"title\":\"GitHub\""));
    assert!(summary.contains("\"subtitle\":\"juan\""));
    assert!(summary.contains("github.com/login"));
    assert!(summary.contains("\"has_totp\":true"));
    assert!(
        !summary.contains("correct horse"),
        "a summary holds no secret"
    );

    let full = session
        .item_content(&vault, &item, &world.key_wrapped, &world.content_enc)
        .unwrap_or_else(|_| panic!("content"));
    assert!(full.contains("correct horse"));

    let code = session
        .item_totp(&vault, &item, &world.key_wrapped, &world.content_enc, 59.0)
        .unwrap_or_else(|_| panic!("totp"));
    assert_eq!(code.len(), 6);
}

#[wasm_bindgen_test]
fn the_desktop_opens_what_the_extension_writes() {
    let world = world();
    let session = unlocked(&world);
    let new_item = Uuid::from_u128(0x0404);
    let json = serde_json::to_string(&content()).unwrap();

    let sealed = session
        .seal_new_item(&VAULT.to_string(), &new_item.to_string(), &json)
        .unwrap_or_else(|_| panic!("seal"));
    assert_eq!(sealed.kind, "login");

    // Read it back the way the desktop does.
    let item_key = wrap::unwrap_key(
        &world.vault_key,
        &contexts::item_key(*VAULT.as_bytes(), *new_item.as_bytes()),
        &sealed.key_wrapped,
    )
    .unwrap();
    let bytes = aead::decrypt(
        &item_key,
        &contexts::item_content(*new_item.as_bytes()),
        &sealed.content_enc,
    )
    .unwrap();
    assert_eq!(ItemContent::decode(&bytes).unwrap(), content());

    // An edit keeps the item key and reads back through the session.
    let mut edited = content();
    edited.title = "GitHub (work)".into();
    let update = session
        .seal_item_update(
            &VAULT.to_string(),
            &new_item.to_string(),
            &sealed.key_wrapped,
            &serde_json::to_string(&edited).unwrap(),
        )
        .unwrap_or_else(|_| panic!("update"));
    assert_eq!(update.key_wrapped, sealed.key_wrapped);
    let summary = session
        .item_summary(
            &VAULT.to_string(),
            &new_item.to_string(),
            &update.key_wrapped,
            &update.content_enc,
        )
        .unwrap_or_else(|_| panic!("summary"));
    assert!(summary.contains("GitHub (work)"));
}

#[wasm_bindgen_test]
fn a_wrong_password_does_not_unlock() {
    let world = world();
    assert!(Session::unlock_with_password(&USER.to_string(), &world.keys_json, "nope").is_err());
    // Another account's id does not unlock this account's keys either.
    assert!(
        Session::unlock_with_password(
            &Uuid::from_u128(9).to_string(),
            &world.keys_json,
            "master password"
        )
        .is_err()
    );
}

#[wasm_bindgen_test]
fn a_restarted_worker_unlocks_with_the_exported_user_key() {
    let world = world();
    let session = unlocked(&world);
    let exported = session.export_user_key();
    assert_eq!(exported, world.user_key.expose().to_vec());

    let mut again = Session::from_user_key(&USER.to_string(), &world.keys_json, &exported)
        .unwrap_or_else(|_| panic!("from user key"));
    again
        .open_vault(&VAULT.to_string(), &world.key_sealed)
        .unwrap_or_else(|_| panic!("open vault"));
    assert!(again.has_vault(&VAULT.to_string()).unwrap_or(false));
    assert!(Session::from_user_key(&USER.to_string(), &world.keys_json, &[7; 32]).is_err());
}

#[wasm_bindgen_test]
fn blobs_do_not_move_between_rows() {
    let world = world();
    let session = unlocked(&world);
    // The same ciphertext presented as another item is refused.
    let other = Uuid::from_u128(0x0505);
    assert!(
        session
            .item_summary(
                &VAULT.to_string(),
                &other.to_string(),
                &world.key_wrapped,
                &world.content_enc
            )
            .is_err()
    );
    // A vault that was never opened cannot be read.
    let mut closed = unlocked(&world);
    closed
        .forget_vault(&VAULT.to_string())
        .unwrap_or_else(|_| panic!("forget"));
    assert!(
        closed
            .item_summary(
                &VAULT.to_string(),
                &ITEM.to_string(),
                &world.key_wrapped,
                &world.content_enc
            )
            .is_err()
    );
}

#[wasm_bindgen_test]
fn a_pin_unlocks_the_same_account_and_a_wrong_one_does_not() {
    let world = world();
    let session = unlocked(&world);
    let blob = session
        .wrap_with_pin("482913")
        .unwrap_or_else(|_| panic!("wrap"));
    // The PIN blob holds neither the PIN nor the raw user key.
    assert!(!blob.windows(6).any(|w| w == b"482913"));
    assert!(
        !blob
            .windows(32)
            .any(|w| w == world.user_key.expose().as_slice())
    );

    let again = Session::unlock_with_pin(&USER.to_string(), &world.keys_json, "482913", &blob)
        .unwrap_or_else(|_| panic!("unlock with pin"));
    assert_eq!(again.export_user_key(), world.user_key.expose().to_vec());
    assert!(
        Session::unlock_with_pin(&USER.to_string(), &world.keys_json, "482914", &blob).is_err()
    );
    assert!(session.wrap_with_pin("12345").is_err());
    assert!(session.wrap_with_pin("12345a").is_err());
    // Bound to the account: another user id does not open it.
    assert!(
        Session::unlock_with_pin(
            &Uuid::from_u128(9).to_string(),
            &world.keys_json,
            "482913",
            &blob
        )
        .is_err()
    );
}

#[wasm_bindgen_test]
fn the_desktop_unlocks_a_paired_extension() {
    let world = world();
    let pairing = kandado_wasm::create_pairing_key().unwrap_or_else(|_| panic!("pairing key"));
    assert_eq!(pairing.code.len(), 6);
    assert_eq!(
        kandado_wasm::pairing_code(&pairing.public_key).unwrap_or_else(|_| panic!("code")),
        pairing.code
    );
    // Frozen: the same digits as the desktop (kandado-crypto's pairing tests).
    assert_eq!(
        kandado_wasm::pairing_code(&[0; 32]).unwrap_or_else(|_| panic!("code")),
        "244942"
    );

    let pairing_id = Uuid::from_u128(0x0606);
    let public: [u8; 32] = pairing.public_key.clone().try_into().unwrap();
    // Sealed the way the desktop app seals it.
    let sealed = kandado_crypto::pairing::seal_user_key(
        *USER.as_bytes(),
        *pairing_id.as_bytes(),
        &PublicKey::from_bytes(public),
        &world.user_key,
    )
    .unwrap();
    let session = Session::from_desktop(
        &USER.to_string(),
        &world.keys_json,
        &pairing.private_key,
        &pairing_id.to_string(),
        &sealed,
    )
    .unwrap_or_else(|_| panic!("from desktop"));
    assert_eq!(session.export_user_key(), world.user_key.expose().to_vec());

    // Another pairing id, or a key that is not this account's, is refused.
    assert!(
        Session::from_desktop(
            &USER.to_string(),
            &world.keys_json,
            &pairing.private_key,
            &Uuid::from_u128(7).to_string(),
            &sealed
        )
        .is_err()
    );
    let stranger = kandado_crypto::pairing::seal_user_key(
        *USER.as_bytes(),
        *pairing_id.as_bytes(),
        &PublicKey::from_bytes(public),
        &SecretKey::generate().unwrap(),
    )
    .unwrap();
    assert!(
        Session::from_desktop(
            &USER.to_string(),
            &world.keys_json,
            &pairing.private_key,
            &pairing_id.to_string(),
            &stranger
        )
        .is_err()
    );
}

const PASSKEY: Uuid = Uuid::from_u128(0x0505);

/// What the worker sends for `navigator.credentials.create`.
const CREATE: &str = r#"{"origin":"https://www.example.com","rp_id":"example.com",
    "rp_name":"Example","user_id":"dXNlci0x","user_name":"juan@example.com",
    "user_display_name":"Juan","challenge":"AAECAwQFBgcICQoLDA0ODw","algorithms":[-7]}"#;

#[wasm_bindgen_test]
fn a_passkey_is_made_and_signs_in_without_its_key_leaving_the_core() {
    let world = world();
    let session = unlocked(&world);
    let (vault, item) = (VAULT.to_string(), PASSKEY.to_string());
    let made = session
        .create_passkey(&vault, &item, CREATE, true)
        .unwrap_or_else(|_| panic!("create"));
    let registration: serde_json::Value = serde_json::from_str(&made.response_json).unwrap();
    let credential_id = registration["credential_id"].as_str().unwrap().to_owned();
    assert!(!made.response_json.contains("private_key"));

    // The desktop reads the same item: a passkey with its key.
    let item_key = wrap::unwrap_key(
        &world.vault_key,
        &contexts::item_key(*VAULT.as_bytes(), *PASSKEY.as_bytes()),
        &made.key_wrapped,
    )
    .unwrap();
    let bytes = aead::decrypt(
        &item_key,
        &contexts::item_content(*PASSKEY.as_bytes()),
        &made.content_enc,
    )
    .unwrap();
    let ItemData::Passkey(stored) = ItemContent::decode(&bytes).unwrap().data else {
        panic!("not a passkey");
    };
    assert_eq!(stored.rp_id, "example.com");
    assert_eq!(stored.credential_id, credential_id);
    assert!(!stored.private_key.is_empty());

    // The summary names the site and the credential; the content hides the key.
    let summary = session
        .item_summary(&vault, &item, &made.key_wrapped, &made.content_enc)
        .unwrap_or_else(|_| panic!("summary"));
    assert!(summary.contains(r#""passkey":{"rp_id":"example.com""#));
    assert!(summary.contains(&credential_id));
    let content = session
        .item_content(&vault, &item, &made.key_wrapped, &made.content_enc)
        .unwrap_or_else(|_| panic!("content"));
    assert!(content.contains(r#""private_key":"""#));
    assert!(!content.contains(stored.private_key.expose()));
    // And JavaScript cannot write a passkey back without its key.
    assert!(
        session
            .seal_item_update(&vault, &item, &made.key_wrapped, &content)
            .is_err()
    );

    let assertion = session
        .assert_passkey(
            &vault,
            &item,
            &made.key_wrapped,
            &made.content_enc,
            r#"{"origin":"https://example.com","challenge":"AAECAwQFBgcICQoLDA0ODw"}"#,
            true,
        )
        .unwrap_or_else(|_| panic!("assert"));
    assert!(assertion.contains(&credential_id));
    assert!(assertion.contains(r#""user_handle":"dXNlci0x""#));
    // Another site, or a login, cannot be signed in with.
    assert!(
        session
            .assert_passkey(
                &vault,
                &item,
                &made.key_wrapped,
                &made.content_enc,
                r#"{"origin":"https://example.org","challenge":"AAECAwQFBgcICQoLDA0ODw"}"#,
                true,
            )
            .is_err()
    );
    assert!(
        session
            .assert_passkey(
                &vault,
                &ITEM.to_string(),
                &world.key_wrapped,
                &world.content_enc,
                r#"{"origin":"https://example.com","challenge":"AAECAwQFBgcICQoLDA0ODw"}"#,
                true,
            )
            .is_err()
    );
}
