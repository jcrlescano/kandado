//! The shared Rust core, callable from the extension's TypeScript.
//!
//! This crate holds no logic of its own. Each function converts its arguments,
//! calls `kandado-crypto` or `kandado-model`, and converts the result, so the
//! extension and the desktop app cannot disagree about a single byte.
//!
//! The unlocked keys live in a [`Session`] inside WebAssembly memory and
//! JavaScript holds only a handle to it. The stateless functions that take and
//! return raw key bytes (`raw`) exist for the shared test vectors and are left
//! out of the extension build.
#![forbid(unsafe_code)]

use kandado_model::{
    generator::{self, Options},
    link, strength,
    totp::Totp,
    url_match::{self, MatchMode, UrlMatch},
};
use wasm_bindgen::prelude::*;

#[cfg(feature = "vectors")]
mod raw;
mod session;

#[cfg(feature = "vectors")]
pub use raw::*;

pub use session::{PairingKey, SealedItem, Session, create_pairing_key, pairing_code};

type JsResult<T> = Result<T, JsError>;

/// A generated password and its entropy.
#[wasm_bindgen(getter_with_clone)]
pub struct GeneratedPassword {
    pub password: String,
    pub bits: f64,
    /// 0 to 4, as the strength meter shows it.
    pub score: u8,
}

fn generated(options: &Options) -> JsResult<GeneratedPassword> {
    let result = generator::generate(options)?;
    Ok(GeneratedPassword {
        password: result.password.expose().to_owned(),
        bits: result.bits,
        score: strength::score_for_bits(result.bits),
    })
}

#[wasm_bindgen(js_name = generateRandomPassword)]
pub fn generate_random_password(
    length: u8,
    uppercase: bool,
    numbers: bool,
    symbols: bool,
) -> JsResult<GeneratedPassword> {
    generated(&Options::Random {
        length,
        uppercase,
        numbers,
        symbols,
    })
}

#[wasm_bindgen(js_name = generateMemorablePassword)]
pub fn generate_memorable_password(
    words: u8,
    capitalise: bool,
    numbers: bool,
    symbols: bool,
) -> JsResult<GeneratedPassword> {
    generated(&Options::Memorable {
        words,
        capitalise,
        numbers,
        symbols,
    })
}

/// The strength score, 0 to 4, of a password a person typed.
#[wasm_bindgen(js_name = passwordScore)]
pub fn password_score(password: &str) -> u8 {
    strength::estimate(password, &[]).score
}

/// The two-factor code for a setup key (an `otpauth://` URI or a bare key) at a time.
#[wasm_bindgen(js_name = totpCode)]
pub fn totp_code(setup: &str, unix_seconds: f64) -> JsResult<String> {
    Ok(Totp::parse(setup)?.code_at(unix_seconds as u64).digits)
}

/// Opens a secure link (D-32) for the viewer page: decrypts `payload` with the
/// key in the link's fragment and returns the snapshot as JSON. Fails the same
/// way for a wrong key, a payload from another link and a changed payload.
#[wasm_bindgen(js_name = openSecureLink)]
pub fn open_secure_link(link_id: &str, fragment: &str, payload: &[u8]) -> JsResult<String> {
    let id = uuid::Uuid::parse_str(link_id).map_err(|_| JsError::new("not a link id"))?;
    let key = link::key_from_fragment(fragment)?;
    let snapshot = link::open(&key, *id.as_bytes(), payload)?;
    serde_json::to_string(&snapshot).map_err(|_| JsError::new("the link could not be read"))
}

/// Whether a saved website matches a page: 0 no, 1 yes, 2 yes but the page is
/// plain HTTP and the login was saved for HTTPS. `mode`: "domain", "host" or "never".
#[wasm_bindgen(js_name = matchUrl)]
pub fn match_url(saved: &str, mode: &str, page: &str) -> JsResult<u8> {
    let mode = match mode {
        "domain" => MatchMode::Domain,
        "host" => MatchMode::Host,
        "never" => MatchMode::Never,
        _ => return Err(JsError::new("unknown match mode")),
    };
    Ok(match url_match::match_url(saved, mode, page) {
        UrlMatch::No => 0,
        UrlMatch::Yes => 1,
        UrlMatch::Insecure => 2,
    })
}
