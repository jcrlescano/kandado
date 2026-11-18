//! Vaults: opening them at unlock, creating and renaming them.
//!
//! Each vault has its own random key. The key is sealed to the account's
//! X25519 public key, the same way it will be sealed to other members when
//! vaults are shared, so nothing changes in the stored form then. The vault's
//! name, colour and icon are encrypted with the vault key.
//!
//! A vault made or renamed here is marked as not yet sent; sync sends it.

use kandado_crypto::{
    Context, SecretKey, account, aead,
    seal::{self, PrivateKey, PublicKey},
};
use kandado_model::{VaultMeta, contexts};
use uuid::Uuid;

use crate::{
    Session, Store, VaultError, items,
    session::AccountKeys,
    store::{LocalAccount, VaultRow},
};

/// A vault as lists and menus show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultInfo {
    pub id: Uuid,
    pub meta: VaultMeta,
    /// Items in the vault, not counting the trash.
    pub item_count: usize,
}

pub(crate) fn key_context(user: Uuid, vault: Uuid) -> Context {
    contexts::vault_key(*user.as_bytes(), *vault.as_bytes())
}

pub(crate) fn meta_context(vault: Uuid) -> Context {
    contexts::vault_meta(*vault.as_bytes())
}

/// Opens a vault key sealed to this account.
pub(crate) fn unseal_key(
    private: &PrivateKey,
    user: Uuid,
    vault: Uuid,
    key_enc: &[u8],
) -> Result<SecretKey, VaultError> {
    let bytes = seal::open(private, &key_context(user, vault), key_enc)?;
    Ok(SecretKey::from_slice(&bytes)?)
}

/// Opens every vault after an unlock: decrypts the account's private key,
/// unseals the vault keys, builds the search index and removes items whose
/// time in the trash has run out.
///
/// A computer that has never synced and has no vault makes a "Personal" one.
/// One that joined an existing account does not: its vaults come with the
/// first pull (see [`Store::has_synced`]).
pub fn open(
    store: &Store,
    session: &mut Session,
    account: &LocalAccount,
    now: i64,
) -> Result<(), VaultError> {
    let user_key = session.user_key().ok_or(VaultError::Locked)?;
    let private =
        account::open_private_key(*account.user_id.as_bytes(), user_key, &account.bundle)?;
    let public = PublicKey::from_bytes(account.bundle.public_x25519);

    session.vault_keys.clear();
    for row in store.vaults()? {
        let key = unseal_key(&private, account.user_id, row.id, &row.key_enc)?;
        session.vault_keys.insert(row.id, key);
    }
    session.account = Some(AccountKeys {
        user_id: account.user_id,
        private,
        public,
    });
    if session.vault_keys.is_empty() && !store.has_synced()? {
        create(
            store,
            session,
            &VaultMeta::new("Personal", "brass", "vault"),
            now,
        )?;
    }
    items::load_index(store, session)?;
    items::purge_expired(store, session, now)?;
    Ok(())
}

/// Creates a vault and returns its id.
pub fn create(
    store: &Store,
    session: &mut Session,
    meta: &VaultMeta,
    now: i64,
) -> Result<Uuid, VaultError> {
    let account = session.account.as_ref().ok_or(VaultError::Locked)?;
    let id = Uuid::now_v7();
    let key = SecretKey::generate()?;
    let row = VaultRow {
        id,
        key_enc: seal::seal(
            &account.public,
            &key_context(account.user_id, id),
            key.expose(),
        )?,
        meta_enc: aead::encrypt(&key, &meta_context(id), &meta.encode()?)?,
        created_at: now,
        updated_at: now,
        server_revision: 0,
        dirty: true,
    };
    store.put_vault(&row)?;
    session.vault_keys.insert(id, key);
    Ok(id)
}

/// Changes a vault's name, colour or icon.
pub fn update(
    store: &Store,
    session: &Session,
    id: Uuid,
    meta: &VaultMeta,
    now: i64,
) -> Result<(), VaultError> {
    let key = session.vault_keys.get(&id).ok_or(VaultError::NotFound)?;
    let mut row = store.vault(id)?.ok_or(VaultError::NotFound)?;
    row.meta_enc = aead::encrypt(key, &meta_context(id), &meta.encode()?)?;
    row.updated_at = now;
    row.dirty = true;
    store.put_vault(&row)?;
    Ok(())
}

/// Every vault, oldest first, with its item count.
pub fn list(store: &Store, session: &Session) -> Result<Vec<VaultInfo>, VaultError> {
    if !session.is_unlocked() {
        return Err(VaultError::Locked);
    }
    store
        .vaults()?
        .into_iter()
        .map(|row| {
            let key = session
                .vault_keys
                .get(&row.id)
                .ok_or(VaultError::NotFound)?;
            let bytes = aead::decrypt(key, &meta_context(row.id), &row.meta_enc)?;
            Ok(VaultInfo {
                id: row.id,
                meta: VaultMeta::decode(&bytes)?,
                item_count: session.index.count(Some(row.id)),
            })
        })
        .collect()
}
