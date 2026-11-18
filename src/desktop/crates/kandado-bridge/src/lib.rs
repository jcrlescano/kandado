//! The local link between the Kandado desktop app and the browser extension.
//!
//! ```text
//! extension ──native messaging (stdio)──► kandado-native-host ──named pipe──► desktop app
//! ```
//!
//! | Module | Does |
//! | --- | --- |
//! | [`protocol`] | The messages and their framing |
//! | [`pipe`] | The per-user named pipe, open to this user only |
//! | [`server`] | The pipe server inside the app; answers through a [`server::Handler`] |
//! | [`relay`] | The native host's work: browser to pipe and back |
//! | [`install`] | Registering the host with Chrome, Edge and Firefox (D-45) |
//!
//! The bridge never serves item data: the extension has its own vault copy. It
//! answers whether the app is unlocked, pairs an extension, and hands a paired
//! extension the user key sealed to it (`kandado_crypto::pairing`).
#![deny(unsafe_code)]

pub mod install;
pub mod pipe;
pub mod protocol;
pub mod relay;
pub mod server;
