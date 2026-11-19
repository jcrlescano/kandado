//! What a Kandado item is, and the logic every client must agree on.
//!
//! No I/O, no clock and no platform APIs: anything that depends on the time
//! takes it as an argument. Builds for Windows, Linux and `wasm32-unknown-unknown`.
//!
//! | Module | Does |
//! | --- | --- |
//! | [`contexts`] | The associated data of vault keys, item keys and content |
//! | [`item`] | The seven item kinds, their fields and the versioned encoding |
//! | [`link`] | A secure link's snapshot of one item, and its encryption |
//! | [`generator`] | Random and memorable passwords |
//! | [`strength`] | Password strength as a 0 to 4 score |
//! | [`totp`] | Two-factor codes (RFC 6238) and `otpauth://` setup keys |
//! | [`url_match`] | Which websites a login belongs to |
//! | [`vault`] | A vault's name, colour and icon |
#![forbid(unsafe_code)]

pub mod contexts;
mod error;
pub mod generator;
pub mod item;
pub mod link;
mod secret;
pub mod strength;
pub mod totp;
pub mod url_match;
pub mod vault;

pub use error::{Error, Result};
pub use item::{ItemContent, ItemData, ItemKind};
pub use secret::Secret;
pub use vault::VaultMeta;
