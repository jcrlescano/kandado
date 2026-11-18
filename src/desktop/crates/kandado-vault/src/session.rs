//! The unlocked state. Holding the user key is what "unlocked" means:
//! locking drops it, and everything decrypted from it goes with it: the
//! vault keys and the search index.

use std::collections::HashMap;

use kandado_crypto::{
    SecretKey,
    seal::{PrivateKey, PublicKey},
};
use uuid::Uuid;

use crate::index::Index;

/// The account's own keys, open while unlocked.
pub(crate) struct AccountKeys {
    pub user_id: Uuid,
    pub private: PrivateKey,
    pub public: PublicKey,
}

/// Holds the user key while the app is unlocked. Keys are wiped from memory
/// when dropped, and cannot be printed.
#[derive(Default)]
pub struct Session {
    user_key: Option<SecretKey>,
    pub(crate) account: Option<AccountKeys>,
    pub(crate) vault_keys: HashMap<Uuid, SecretKey>,
    pub(crate) index: Index,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn unlock(&mut self, user_key: SecretKey) {
        self.lock();
        self.user_key = Some(user_key);
    }

    /// Drops the user key and everything decrypted with it.
    pub fn lock(&mut self) {
        self.user_key = None;
        self.account = None;
        self.vault_keys.clear();
        self.index = Index::default();
    }

    pub fn is_unlocked(&self) -> bool {
        self.user_key.is_some()
    }

    /// The user key, for the moment something needs it. Do not keep the borrow.
    pub fn user_key(&self) -> Option<&SecretKey> {
        self.user_key.as_ref()
    }

    /// The search index over the open vaults. Empty while locked.
    pub fn index(&self) -> &Index {
        &self.index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locking_drops_the_key() {
        let mut session = Session::new();
        assert!(!session.is_unlocked());
        session.unlock(SecretKey::generate().unwrap());
        assert!(session.is_unlocked());
        assert!(session.user_key().is_some());
        session.lock();
        assert!(!session.is_unlocked());
        assert!(session.user_key().is_none());
    }

    #[test]
    fn locking_drops_vault_keys_and_the_index() {
        let mut session = Session::new();
        session.unlock(SecretKey::generate().unwrap());
        session
            .vault_keys
            .insert(Uuid::from_u128(1), SecretKey::generate().unwrap());
        session.lock();
        assert!(session.vault_keys.is_empty());
        assert!(session.account.is_none());
        assert!(session.index().is_empty());
    }
}
