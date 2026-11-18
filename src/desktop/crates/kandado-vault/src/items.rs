//! Items: create, edit, move, trash, restore, delete, favourite, duplicate.
//!
//! Each item has its own random key, wrapped by its vault's key; the content
//! is encrypted with the item key. Moving an item wraps its key again under
//! the new vault's key and re-encrypts nothing else. Every write updates the
//! store and the search index together, and marks the item as not yet sent:
//! sync sends it (see [`crate::sync`]).
//!
//! An item's content has a version id. A real edit gets a new one and keeps
//! the old content in the item's history; the star, the trash and a move do
//! not, because they are not edits of what the person wrote.

use kandado_crypto::{Context, SecretKey, aead, wrap};
use kandado_model::{ItemContent, contexts};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    Session, Store, VaultError,
    index::Entry,
    store::{ItemRow, Tombstone},
};

/// How long an item stays in the trash before it is deleted for good.
pub const TRASH_DAYS: i64 = 30;
const DAY: i64 = 24 * 60 * 60;

/// A decrypted item. Drop it as soon as the screen that asked is done.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: Uuid,
    pub vault_id: Uuid,
    pub content: ItemContent,
    /// Names this saved version of the content.
    pub version_id: Uuid,
    /// Unix seconds.
    pub created_at: i64,
    pub updated_at: i64,
    pub trashed_at: Option<i64>,
}

pub(crate) fn key_context(vault: Uuid, item: Uuid) -> Context {
    contexts::item_key(*vault.as_bytes(), *item.as_bytes())
}

fn content_context(item: Uuid) -> Context {
    contexts::item_content(*item.as_bytes())
}

/// SHA-256, to tell whether content changed without keeping a second copy.
pub(crate) fn digest(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}

fn vault_key(session: &Session, vault: Uuid) -> Result<&SecretKey, VaultError> {
    if !session.is_unlocked() {
        return Err(VaultError::Locked);
    }
    session.vault_keys.get(&vault).ok_or(VaultError::NotFound)
}

pub(crate) fn item_key(session: &Session, row: &ItemRow) -> Result<SecretKey, VaultError> {
    let vault = vault_key(session, row.vault_id)?;
    Ok(wrap::unwrap_key(
        vault,
        &key_context(row.vault_id, row.id),
        &row.key_enc,
    )?)
}

fn encrypt(key: &SecretKey, id: Uuid, content: &ItemContent) -> Result<Vec<u8>, VaultError> {
    Ok(aead::encrypt(
        key,
        &content_context(id),
        &content.encode()?,
    )?)
}

pub(crate) fn decrypt(key: &SecretKey, id: Uuid, blob: &[u8]) -> Result<ItemContent, VaultError> {
    let bytes = aead::decrypt(key, &content_context(id), blob)?;
    Ok(ItemContent::decode(&bytes)?)
}

fn row(store: &Store, id: Uuid) -> Result<ItemRow, VaultError> {
    store.item(id)?.ok_or(VaultError::NotFound)
}

/// The item's row and key, for the history and sync modules.
pub(crate) fn row_and_key(
    store: &Store,
    session: &Session,
    id: Uuid,
) -> Result<(ItemRow, SecretKey), VaultError> {
    let row = row(store, id)?;
    let key = item_key(session, &row)?;
    Ok((row, key))
}

pub(crate) fn entry(row: &ItemRow, content: &ItemContent) -> Entry {
    Entry::new(
        row.id,
        row.vault_id,
        content,
        row.created_at,
        row.updated_at,
        row.trashed_at,
    )
}

/// Builds the search index from the store. An item that cannot be read (from
/// a newer version of Kandado, say) is left out and logged by id only.
pub(crate) fn load_index(store: &Store, session: &mut Session) -> Result<(), VaultError> {
    let mut entries = Vec::new();
    for row in store.items()? {
        let content =
            item_key(session, &row).and_then(|key| decrypt(&key, row.id, &row.content_enc));
        match content {
            Ok(content) => {
                entries.push(entry(&row, &content));
            }
            Err(error) => tracing::warn!(item = %row.id, %error, "an item could not be read"),
        }
    }
    session.index = Default::default();
    for entry in entries {
        session.index.upsert(entry);
    }
    Ok(())
}

/// Creates an item in `vault` and returns its id.
pub fn create(
    store: &Store,
    session: &mut Session,
    vault: Uuid,
    content: &ItemContent,
    now: i64,
) -> Result<Uuid, VaultError> {
    let id = Uuid::now_v7();
    let key = SecretKey::generate()?;
    let row = ItemRow {
        id,
        vault_id: vault,
        key_enc: wrap::wrap_key(vault_key(session, vault)?, &key_context(vault, id), &key)?,
        content_enc: encrypt(&key, id, content)?,
        version_id: Uuid::now_v7(),
        created_at: now,
        updated_at: now,
        trashed_at: None,
        server_revision: 0,
        dirty: true,
        base: None,
    };
    store.put_item(&row)?;
    session.index.upsert(entry(&row, content));
    Ok(id)
}

/// Decrypts one item.
pub fn get(store: &Store, session: &Session, id: Uuid) -> Result<Item, VaultError> {
    let (row, key) = row_and_key(store, session, id)?;
    Ok(Item {
        id,
        vault_id: row.vault_id,
        content: decrypt(&key, id, &row.content_enc)?,
        version_id: row.version_id,
        created_at: row.created_at,
        updated_at: row.updated_at,
        trashed_at: row.trashed_at,
    })
}

/// Saves an edit. The version it replaces goes to the item's history. Saving
/// without changing anything does nothing.
pub fn update(
    store: &Store,
    session: &mut Session,
    id: Uuid,
    content: &ItemContent,
    now: i64,
) -> Result<(), VaultError> {
    let (mut row, key) = row_and_key(store, session, id)?;
    if &decrypt(&key, id, &row.content_enc)? == content {
        return Ok(());
    }
    let previous = (row.version_id, row.content_enc.clone(), row.updated_at);
    row.content_enc = encrypt(&key, id, content)?;
    row.version_id = Uuid::now_v7();
    row.updated_at = now;
    row.dirty = true;
    store.atomically(|store| {
        store.add_revision(id, previous.0, &previous.1, previous.2)?;
        store.put_item(&row)
    })?;
    session.index.upsert(entry(&row, content));
    Ok(())
}

/// Adds or removes the star. This is not an edit worth a history entry.
pub fn set_favourite(
    store: &Store,
    session: &mut Session,
    id: Uuid,
    favourite: bool,
    now: i64,
) -> Result<(), VaultError> {
    let (mut row, key) = row_and_key(store, session, id)?;
    let mut content = decrypt(&key, id, &row.content_enc)?;
    if content.favourite == favourite {
        return Ok(());
    }
    content.favourite = favourite;
    row.content_enc = encrypt(&key, id, &content)?;
    row.updated_at = now;
    row.dirty = true;
    store.put_item(&row)?;
    session.index.upsert(entry(&row, &content));
    Ok(())
}

/// Moves items to another vault. All of them move or none does.
pub fn move_to(
    store: &Store,
    session: &mut Session,
    ids: &[Uuid],
    vault: Uuid,
    now: i64,
) -> Result<(), VaultError> {
    let target = vault_key(session, vault)?;
    let mut moves = Vec::new();
    for &id in ids {
        let (mut row, key) = row_and_key(store, session, id)?;
        if row.vault_id != vault {
            row.key_enc = wrap::wrap_key(target, &key_context(vault, id), &key)?;
            row.vault_id = vault;
            row.updated_at = now;
            row.dirty = true;
            moves.push(row);
        }
    }
    store.atomically(|store| {
        for row in &moves {
            store.put_item(row)?;
        }
        Ok(())
    })?;
    for row in moves {
        if let Some(entry) = session.index.get_mut(row.id) {
            entry.vault_id = vault;
            entry.updated_at = now;
        }
    }
    Ok(())
}

fn set_trashed(
    store: &Store,
    session: &mut Session,
    ids: &[Uuid],
    trashed_at: Option<i64>,
) -> Result<(), VaultError> {
    if !session.is_unlocked() {
        return Err(VaultError::Locked);
    }
    store.atomically(|store| {
        for &id in ids {
            let mut row = store.item(id)?.ok_or(crate::StoreError::NotFound)?;
            if row.trashed_at != trashed_at {
                row.trashed_at = trashed_at;
                row.dirty = true;
                store.put_item(&row)?;
            }
        }
        Ok(())
    })?;
    for &id in ids {
        if let Some(entry) = session.index.get_mut(id) {
            entry.trashed_at = trashed_at;
        }
    }
    Ok(())
}

/// Moves items to the trash, where they stay for [`TRASH_DAYS`] days.
pub fn trash(
    store: &Store,
    session: &mut Session,
    ids: &[Uuid],
    now: i64,
) -> Result<(), VaultError> {
    set_trashed(store, session, ids, Some(now))
}

/// Takes items out of the trash, back to the vault they were in.
pub fn restore(store: &Store, session: &mut Session, ids: &[Uuid]) -> Result<(), VaultError> {
    set_trashed(store, session, ids, None)
}

/// Deletes items for good, with their history. Only items already in the
/// trash can be deleted, so nothing skips the 30 days by accident. An item
/// the server has leaves a tombstone, which sync sends.
pub fn delete_forever(
    store: &Store,
    session: &mut Session,
    ids: &[Uuid],
) -> Result<(), VaultError> {
    if !session.is_unlocked() {
        return Err(VaultError::Locked);
    }
    let mut rows = Vec::new();
    for &id in ids {
        let row = row(store, id)?;
        if row.trashed_at.is_none() {
            return Err(VaultError::NotInTrash);
        }
        rows.push(row);
    }
    store.atomically(|store| {
        for row in &rows {
            if row.server_revision > 0 {
                store.put_tombstone(&Tombstone {
                    item_id: row.id,
                    vault_id: row.vault_id,
                    server_revision: row.server_revision,
                })?;
            }
            store.delete_item(row.id)?;
        }
        Ok(())
    })?;
    for &id in ids {
        session.index.remove(id);
    }
    Ok(())
}

/// Deletes everything in the trash and returns how many items that was.
pub fn empty_trash(store: &Store, session: &mut Session) -> Result<usize, VaultError> {
    let ids: Vec<Uuid> = session
        .index
        .list(crate::index::Filter {
            trash: true,
            ..Default::default()
        })
        .iter()
        .map(|entry| entry.id)
        .collect();
    delete_forever(store, session, &ids)?;
    Ok(ids.len())
}

/// Deletes items that have been in the trash for [`TRASH_DAYS`] days.
pub fn purge_expired(store: &Store, session: &mut Session, now: i64) -> Result<usize, VaultError> {
    let ids: Vec<Uuid> = session
        .index
        .list(crate::index::Filter {
            trash: true,
            ..Default::default()
        })
        .iter()
        .filter(|entry| days_left(entry.trashed_at.unwrap_or(now), now) == 0)
        .map(|entry| entry.id)
        .collect();
    delete_forever(store, session, &ids)?;
    Ok(ids.len())
}

/// Whole days until a trashed item is deleted for good: 30 just after
/// trashing, 0 when its time is up.
pub fn days_left(trashed_at: i64, now: i64) -> i64 {
    let remaining = trashed_at + TRASH_DAYS * DAY - now;
    if remaining <= 0 {
        0
    } else {
        (remaining + DAY - 1) / DAY
    }
}

/// Copies an item into the same vault, with its own key and no history.
pub fn duplicate(
    store: &Store,
    session: &mut Session,
    id: Uuid,
    now: i64,
) -> Result<Uuid, VaultError> {
    let item = get(store, session, id)?;
    let mut content = item.content;
    content.title = format!("{} (copy)", content.title);
    content.favourite = false;
    create(store, session, item.vault_id, &content, now)
}
