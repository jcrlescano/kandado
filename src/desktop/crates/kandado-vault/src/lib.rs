//! Kandado's local vault: the encrypted store, the unlocked session, the
//! vault and item operations, and the client for the backend. No UI and no
//! platform APIs, so everything here is testable without a window.
//!
//! | Module | Does |
//! | --- | --- |
//! | [`api`] | `reqwest` client for `kandado-protocol`, refreshing tokens itself |
//! | [`store`] | SQLite file: the account, vaults, items and their history, all as ciphertext |
//! | [`session`] | The unlocked keys and the search index, in memory only; `lock()` drops them |
//! | [`vaults`] | Open the vaults at unlock; create and rename them |
//! | [`items`] | Create, edit, move, trash, restore, delete, favourite, duplicate |
//! | [`index`] | Search over decrypted titles, usernames, websites and notes |
//! | [`history`] | An item's earlier versions and restoring one |
//! | [`sync`] | Pull, push and merge against the server; the offline outbox |
//! | [`events`] | The connection that says when the server has something new |
//!
//! Realtime and the scheduler that drives sync live in `kandado-app`.
#![forbid(unsafe_code)]

pub mod api;
pub mod events;
pub mod history;
pub mod import;
pub mod index;
pub mod items;
pub mod session;
pub mod store;
pub mod sync;
pub mod vaults;

pub use api::{Api, ApiError, Tokens};
pub use session::Session;
pub use store::{LocalAccount, Store, StoreError};

/// Why a vault or item operation failed.
#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error(transparent)]
    Store(StoreError),
    #[error("the vault data could not be decrypted: {0}")]
    Crypto(#[from] kandado_crypto::Error),
    #[error("the vault data could not be read: {0}")]
    Model(#[from] kandado_model::Error),
    #[error("the vault is locked")]
    Locked,
    #[error("that vault or item is not on this computer")]
    NotFound,
    #[error("the import was cancelled")]
    Cancelled,
    #[error("only items in the trash can be deleted forever")]
    NotInTrash,
}

impl From<StoreError> for VaultError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::NotFound => Self::NotFound,
            other => Self::Store(other),
        }
    }
}
