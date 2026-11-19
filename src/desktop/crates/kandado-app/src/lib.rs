//! Application services for the Kandado desktop app: Google sign-in, the key
//! flows, locking, settings, and the vault as views need it. No UI; the host
//! calls [`KandadoApp`].
//!
//! | Module | Does |
//! | --- | --- |
//! | [`service`] | [`KandadoApp`]: every user action as a method |
//! | [`vault`] | Vaults, items, search and history as view models; reveal and copy |
//! | [`links`] | Secure links: one item shared through a link that expires |
//! | [`clipboard`] | Copy with timed clear, kept out of clipboard history |
//! | [`totp_import`] | Two-factor setup keys, pasted or from a QR code on screen |
//! | [`auth`] | Loopback listener for the Google redirect |
//! | [`browser`] | The browser extension link: pairing and unlock (D-45) |
//! | [`monitor`] | Health: weak, reused, exposed and breached passwords, two-factor |
//! | [`lock`] | Idle timer and the wrong-password limit |
//! | [`device_key`] | Windows Hello / DPAPI key that unlocks without the password |
//! | [`secrets`] | Credential Manager for the refresh token |
//! | [`settings`] | `settings.json` |
//! | [`update`] | Signed updates: check, download, verify, install (D-54) |
//! | [`tasks`] | Runs async work for views that have their own executor |
//! | [`platform`] | Win32: open a URL, watch for sleep and session changes |
//! | [`profile`] | Installed or development build: which data folder and names |
#![deny(unsafe_code)]

pub mod auth;
pub mod browser;
pub mod clipboard;
pub mod device_key;
pub mod links;
pub mod lock;
pub mod monitor;
pub mod platform;
pub mod profile;
pub mod secrets;
pub mod service;
pub mod settings;
pub mod sync;
pub mod tasks;
#[doc(hidden)]
pub mod testing;
pub mod totp_import;
pub mod transfer;
pub mod update;
pub mod vault;

pub use kandado_crypto::kdf::KdfParams;
pub use kandado_vault::import::{ExportFormat, ImportError, ImportReport, Skipped, Source};
pub use kandado_vault::{
    ApiError, VaultError,
    index::{Entry, Filter},
    items::Item,
    vaults::VaultInfo,
};
pub use service::{AppError, KandadoApp, Outcome, Paths, Phase, UnlockError};
