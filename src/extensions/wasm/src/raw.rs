//! The stateless crypto functions the shared test vectors call directly.
//!
//! They take and return raw key bytes, so the extension build leaves them out
//! (it builds with `--no-default-features`). Keys live in [`crate::Session`].
//! The feature is on by default so the vector tests, which run in a browser,
//! can reach them.

use kandado_crypto::{
    Context, Purpose, SecretKey, aead,
    kdf::{self, KdfParams},
    recovery::RecoveryPhrase,
    seal, wrap,
};
use wasm_bindgen::prelude::*;

type JsResult<T> = Result<T, JsError>;

fn context(purpose: u8, owner: &[u8], object: &[u8], format: u8) -> JsResult<Context> {
    let id = |bytes: &[u8]| {
        bytes
            .try_into()
            .map_err(|_| JsError::new("an ID must be 16 bytes"))
    };
    Ok(Context::new(
        Purpose::try_from(purpose)?,
        id(owner)?,
        id(object)?,
        format,
    ))
}

/// The associated data for a blob. Exposed so tests can check the encoding.
#[wasm_bindgen(js_name = contextBytes)]
pub fn context_bytes(purpose: u8, owner: &[u8], object: &[u8], format: u8) -> JsResult<Vec<u8>> {
    Ok(context(purpose, owner, object, format)?.to_bytes().to_vec())
}

#[wasm_bindgen(js_name = aeadEncrypt)]
pub fn aead_encrypt(
    key: &[u8],
    purpose: u8,
    owner: &[u8],
    object: &[u8],
    format: u8,
    plaintext: &[u8],
) -> JsResult<Vec<u8>> {
    let key = SecretKey::from_slice(key)?;
    Ok(aead::encrypt(
        &key,
        &context(purpose, owner, object, format)?,
        plaintext,
    )?)
}

#[wasm_bindgen(js_name = aeadDecrypt)]
pub fn aead_decrypt(
    key: &[u8],
    purpose: u8,
    owner: &[u8],
    object: &[u8],
    format: u8,
    blob: &[u8],
) -> JsResult<Vec<u8>> {
    let key = SecretKey::from_slice(key)?;
    Ok(aead::decrypt(&key, &context(purpose, owner, object, format)?, blob)?.to_vec())
}

/// Argon2id: master password to master key.
#[wasm_bindgen(js_name = deriveMasterKey)]
pub fn derive_master_key(
    password: &str,
    salt: &[u8],
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
) -> JsResult<Vec<u8>> {
    let salt = salt
        .try_into()
        .map_err(|_| JsError::new("the salt must be 16 bytes"))?;
    let params = KdfParams {
        memory_kib,
        iterations,
        parallelism,
    };
    Ok(kdf::derive_master_key(password.as_bytes(), salt, &params)?
        .expose()
        .to_vec())
}

#[wasm_bindgen(js_name = wrapKey)]
pub fn wrap_key(
    wrapping_key: &[u8],
    purpose: u8,
    owner: &[u8],
    object: &[u8],
    format: u8,
    key: &[u8],
) -> JsResult<Vec<u8>> {
    Ok(wrap::wrap_key(
        &SecretKey::from_slice(wrapping_key)?,
        &context(purpose, owner, object, format)?,
        &SecretKey::from_slice(key)?,
    )?)
}

#[wasm_bindgen(js_name = unwrapKey)]
pub fn unwrap_key(
    wrapping_key: &[u8],
    purpose: u8,
    owner: &[u8],
    object: &[u8],
    format: u8,
    wrapped: &[u8],
) -> JsResult<Vec<u8>> {
    let key = wrap::unwrap_key(
        &SecretKey::from_slice(wrapping_key)?,
        &context(purpose, owner, object, format)?,
        wrapped,
    )?;
    Ok(key.expose().to_vec())
}

#[wasm_bindgen(js_name = publicKeyFor)]
pub fn public_key_for(private_key: &[u8]) -> JsResult<Vec<u8>> {
    Ok(seal::PrivateKey::from_slice(private_key)?
        .public_key()?
        .as_bytes()
        .to_vec())
}

/// Encrypts to a public key.
#[wasm_bindgen(js_name = sealTo)]
pub fn seal_to(
    public_key: &[u8],
    purpose: u8,
    owner: &[u8],
    object: &[u8],
    format: u8,
    plaintext: &[u8],
) -> JsResult<Vec<u8>> {
    let public = seal::PublicKey::from_slice(public_key)?;
    Ok(seal::seal(
        &public,
        &context(purpose, owner, object, format)?,
        plaintext,
    )?)
}

#[wasm_bindgen(js_name = openSealed)]
pub fn open_sealed(
    private_key: &[u8],
    purpose: u8,
    owner: &[u8],
    object: &[u8],
    format: u8,
    blob: &[u8],
) -> JsResult<Vec<u8>> {
    let private = seal::PrivateKey::from_slice(private_key)?;
    Ok(seal::open(&private, &context(purpose, owner, object, format)?, blob)?.to_vec())
}

/// The key a recovery phrase unwraps the user key with.
#[wasm_bindgen(js_name = recoveryKey)]
pub fn recovery_key(phrase: &str) -> JsResult<Vec<u8>> {
    Ok(RecoveryPhrase::parse(phrase)?
        .recovery_key()?
        .expose()
        .to_vec())
}
