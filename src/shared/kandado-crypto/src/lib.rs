//! Kandado's key hierarchy and every operation that touches key material.
//!
//! This is the only crate allowed to handle keys. It has no I/O beyond the
//! system random source, no async runtime and no platform APIs, and it builds
//! for Windows, Linux and `wasm32-unknown-unknown`.
//!
//! Every blob this crate produces starts with a version byte naming the
//! algorithm suite, so suites can be replaced without guessing at old data.
//!
//! | Module | Does |
//! | --- | --- |
//! | [`account`] | Account setup: user key, recovery, master password change |
//! | [`aead`] | XChaCha20-Poly1305 with a random nonce |
//! | [`context`] | Associated data binding a blob to its purpose, owner and object |
//! | [`kdf`] | Argon2id: master password to master key |
//! | [`wrap`] | One symmetric key encrypted under another |
//! | [`seal`] | Encrypt to an X25519 public key (HPKE, RFC 9180) |
//! | [`passkey`] | ES256 keys for WebAuthn passkeys: generate, store, sign |
//! | [`pairing`] | Pairing the browser extension; the user key sealed to it |
//! | [`recovery`] | The 12-word recovery phrase and its key |
//! | [`random`] | The system's secure random source |
#![forbid(unsafe_code)]

pub mod account;
pub mod aead;
pub mod context;
mod error;
pub mod kdf;
pub mod pairing;
pub mod passkey;
pub mod random;
pub mod recovery;
pub mod seal;
mod secret;
pub mod wrap;

pub use context::{Context, Purpose};
pub use error::{Error, Result};
pub use secret::{KEY_LEN, SecretBytes, SecretKey};
