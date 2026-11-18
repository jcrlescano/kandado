//! Runs `kandado-crypto/vectors/v1.json` through the WebAssembly build, in a
//! browser. The native crate checks the same file, so passing here proves the
//! extension and the desktop app read exactly the same bytes.
//!
//! `cargo test -p kandado-wasm --target wasm32-unknown-unknown`
#![cfg(target_arch = "wasm32")]

use kandado_wasm as core;
use serde::Deserialize;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

const VECTORS: &str = include_str!("../../../shared/kandado-crypto/vectors/v1.json");

#[derive(Deserialize)]
struct Vectors {
    context: Vec<ContextVector>,
    aead: Vec<AeadVector>,
    kdf: Vec<KdfVector>,
    wrap: Vec<WrapVector>,
    seal: Vec<SealVector>,
    recovery: Vec<RecoveryVector>,
}

#[derive(Deserialize)]
struct ContextFields {
    purpose: u8,
    owner: String,
    object: String,
    format: u8,
}

#[derive(Deserialize)]
struct ContextVector {
    #[serde(flatten)]
    fields: ContextFields,
    bytes: String,
}

#[derive(Deserialize)]
struct AeadVector {
    key: String,
    context: ContextFields,
    plaintext: String,
    blob: String,
}

#[derive(Deserialize)]
struct KdfVector {
    password: String,
    salt: String,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    key: String,
}

#[derive(Deserialize)]
struct WrapVector {
    wrapping_key: String,
    context: ContextFields,
    key: String,
    blob: String,
}

#[derive(Deserialize)]
struct SealVector {
    private_key: String,
    public_key: String,
    context: ContextFields,
    plaintext: String,
    blob: String,
}

#[derive(Deserialize)]
struct RecoveryVector {
    phrase: String,
    key: String,
}

fn load() -> Vectors {
    serde_json::from_str(VECTORS).unwrap()
}

fn bytes(hex_text: &str) -> Vec<u8> {
    hex::decode(hex_text).unwrap()
}

/// `JsError` has no `Debug`, so `unwrap` is not available on these results.
fn ok<T>(result: Result<T, JsError>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{:?}", JsValue::from(error)),
    }
}

use wasm_bindgen::{JsError, JsValue};

#[wasm_bindgen_test]
fn context_encoding() {
    let vectors = load().context;
    assert!(!vectors.is_empty());
    for v in vectors {
        let f = &v.fields;
        let encoded = ok(core::context_bytes(
            f.purpose,
            &bytes(&f.owner),
            &bytes(&f.object),
            f.format,
        ));
        assert_eq!(hex::encode(encoded), v.bytes);
    }
}

#[wasm_bindgen_test]
fn aead_decrypts_known_blobs() {
    let vectors = load().aead;
    assert!(!vectors.is_empty());
    for v in vectors {
        let c = &v.context;
        let plaintext = ok(core::aead_decrypt(
            &bytes(&v.key),
            c.purpose,
            &bytes(&c.owner),
            &bytes(&c.object),
            c.format,
            &bytes(&v.blob),
        ));
        assert_eq!(hex::encode(plaintext), v.plaintext);
    }
}

#[wasm_bindgen_test]
fn kdf_derives_known_keys() {
    let vectors = load().kdf;
    assert!(!vectors.is_empty());
    for v in vectors {
        let key = ok(core::derive_master_key(
            &v.password,
            &bytes(&v.salt),
            v.memory_kib,
            v.iterations,
            v.parallelism,
        ));
        assert_eq!(hex::encode(key), v.key);
    }
}

#[wasm_bindgen_test]
fn wrap_unwraps_known_blobs() {
    let vectors = load().wrap;
    assert!(!vectors.is_empty());
    for v in vectors {
        let c = &v.context;
        let key = ok(core::unwrap_key(
            &bytes(&v.wrapping_key),
            c.purpose,
            &bytes(&c.owner),
            &bytes(&c.object),
            c.format,
            &bytes(&v.blob),
        ));
        assert_eq!(hex::encode(key), v.key);
    }
}

#[wasm_bindgen_test]
fn seal_opens_known_blobs() {
    let vectors = load().seal;
    assert!(!vectors.is_empty());
    for v in vectors {
        let c = &v.context;
        let private = bytes(&v.private_key);
        assert_eq!(
            hex::encode(ok(core::public_key_for(&private))),
            v.public_key
        );
        let plaintext = ok(core::open_sealed(
            &private,
            c.purpose,
            &bytes(&c.owner),
            &bytes(&c.object),
            c.format,
            &bytes(&v.blob),
        ));
        assert_eq!(hex::encode(plaintext), v.plaintext);
    }
}

#[wasm_bindgen_test]
fn recovery_phrases_give_known_keys() {
    let vectors = load().recovery;
    assert!(!vectors.is_empty());
    for v in vectors {
        assert_eq!(hex::encode(ok(core::recovery_key(&v.phrase))), v.key);
    }
}

/// The browser's random source works, and what one build encrypts it can decrypt.
#[wasm_bindgen_test]
fn encrypts_with_browser_randomness() {
    let key = [7u8; 32];
    let id = [1u8; 16];
    let a = ok(core::aead_encrypt(&key, 8, &id, &id, 1, b"secret"));
    let b = ok(core::aead_encrypt(&key, 8, &id, &id, 1, b"secret"));
    assert_ne!(a, b);
    assert_eq!(ok(core::aead_decrypt(&key, 8, &id, &id, 1, &a)), b"secret");

    let generated = ok(core::generate_random_password(20, true, true, true));
    assert_eq!(generated.password.len(), 20);
    assert_eq!(generated.score, 4);
}

#[wasm_bindgen_test]
fn model_functions_answer_as_they_do_natively() {
    // RFC 6238 appendix B, SHA-1, at 59 seconds, 8 digits.
    let setup = "otpauth://totp/x?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&digits=8";
    assert_eq!(ok(core::totp_code(setup, 59.0)), "94287082");
    assert_eq!(core::password_score("password"), 1);
    assert_eq!(
        ok(core::match_url(
            "https://google.com",
            "domain",
            "https://accounts.google.com"
        )),
        1
    );
    assert_eq!(
        ok(core::match_url(
            "https://alice.github.io",
            "domain",
            "https://mallory.github.io"
        )),
        0
    );
    assert_eq!(
        ok(core::match_url(
            "https://example.com",
            "domain",
            "http://example.com"
        )),
        2
    );
}
