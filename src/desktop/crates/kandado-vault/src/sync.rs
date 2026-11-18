//! Sync: keeping this computer's vault the same as the server's, and so the
//! same as the person's other computers.
//!
//! Every write made here marks its row as not yet sent (the "outbox" is those
//! rows and the tombstones of items deleted for good). A sync round then:
//!
//! 1. **pulls** everything the server has after this computer's cursor and
//!    applies it, merging with anything changed here;
//! 2. **pushes** what is waiting. The server accepts a change only if it was
//!    made against the row's current revision; otherwise it answers with the
//!    row as it is now, which is merged in and sent again.
//!
//! The merge is three-way. For each item this computer remembers what the
//! server had last time (the *base*). Content, vault and trash state are each
//! compared with it: a part changed only here is kept, a part changed only
//! elsewhere is taken, and a part changed in both goes to this computer's
//! change, which is the later writer. The version that lost is kept in the
//! item's history, here and on the server, so no edit is ever lost (D-16).
//! An item deleted for good is deleted everywhere, whatever else happened to it.
//!
//! The network is behind [`Remote`] and the locked store behind [`Access`], so
//! no lock is held while waiting on the network, and the whole algorithm runs
//! against an in-memory server in the tests.

use std::{collections::HashSet, future::Future};

use kandado_model::{ItemContent, ItemKind};
use kandado_protocol::{
    ChangeResult, MAX_BATCH, Opaque, PullResponse, PushRequest, PushResponse, SyncItem, SyncVault,
};
use uuid::Uuid;

use crate::{
    ApiError, Session, Store, StoreError, VaultError,
    items::{self, digest},
    store::{Base, ItemRow, Tombstone, VaultRow},
    vaults,
};

/// How many pull-and-push rounds one sync makes at most. Each round that
/// pushes anything has fixed what conflicted in the last; more than a handful
/// means two computers are changing the same things at the same moment.
const MAX_ROUNDS: usize = 8;

/// Rows asked for in one pull.
const PULL_LIMIT: u32 = 200;

/// The server, as sync needs it.
pub trait Remote: Send + Sync {
    fn pull(
        &self,
        cursor: i64,
        limit: u32,
    ) -> impl Future<Output = Result<PullResponse, ApiError>> + Send;
    fn push(
        &self,
        request: &PushRequest,
    ) -> impl Future<Output = Result<PushResponse, ApiError>> + Send;
}

impl Remote for crate::Api {
    async fn pull(&self, cursor: i64, limit: u32) -> Result<PullResponse, ApiError> {
        self.sync_pull(cursor, limit).await
    }

    async fn push(&self, request: &PushRequest) -> Result<PushResponse, ApiError> {
        self.sync_push(request).await
    }
}

/// The store and the session, locked for the length of one closure. The
/// closure is synchronous, so a lock is never held across an `await`.
pub trait Access: Send + Sync {
    fn with<T>(
        &self,
        work: impl FnOnce(&Store, &mut Session) -> Result<T, VaultError>,
    ) -> Result<T, VaultError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error(transparent)]
    Vault(#[from] VaultError),
}

impl From<StoreError> for SyncError {
    fn from(error: StoreError) -> Self {
        Self::Vault(error.into())
    }
}

/// What a sync did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Report {
    /// Vaults and items taken from the server.
    pub pulled: usize,
    /// Vaults and items sent and accepted.
    pub pushed: usize,
    /// Changes that met a newer version on the server and were merged.
    pub conflicts: usize,
    /// Changes the server refused. They stay here and are not retried until
    /// the next sync.
    pub rejected: usize,
    /// Something visible changed here: lists need drawing again.
    pub changed: bool,
}

/// What a pull changed, for the search index.
enum Touch {
    Upsert(Uuid),
    Remove(Uuid),
}

// ---- the row an item has on the wire -------------------------------------

fn base_of(content_enc: &[u8], vault_id: Uuid, trashed_at: Option<i64>) -> Base {
    Base {
        digest: digest(content_enc),
        vault_id,
        trashed_at,
    }
}

/// The row a remote item makes when nothing here competes with it.
fn adopt(remote: &SyncItem, local: Option<&ItemRow>) -> ItemRow {
    ItemRow {
        id: remote.id,
        vault_id: remote.vault_id,
        key_enc: remote.key_wrapped.as_bytes().to_vec(),
        content_enc: remote.content_enc.as_bytes().to_vec(),
        version_id: remote.version_id,
        created_at: local.map_or(remote.created_at, |row| {
            row.created_at.min(remote.created_at)
        }),
        updated_at: remote.updated_at,
        trashed_at: remote.trashed_at,
        server_revision: remote.revision,
        dirty: false,
        base: Some(base_of(
            remote.content_enc.as_bytes(),
            remote.vault_id,
            remote.trashed_at,
        )),
    }
}

/// Merges an item changed here with a newer version from the server. Returns
/// the row to keep, which is marked as not yet sent if it still differs from
/// the server's.
fn merge(local: &ItemRow, remote: &SyncItem) -> ItemRow {
    let base = local.base.as_ref();
    let content_changed = base.is_none_or(|base| digest(&local.content_enc) != base.digest);
    let place_changed = base.is_none_or(|base| local.vault_id != base.vault_id);
    let trash_changed = base.is_none_or(|base| local.trashed_at != base.trashed_at);

    let mut merged = adopt(remote, Some(local));
    if content_changed {
        merged.content_enc = local.content_enc.clone();
        merged.version_id = local.version_id;
        merged.updated_at = local.updated_at;
    }
    if place_changed {
        merged.vault_id = local.vault_id;
        merged.key_enc = local.key_enc.clone();
    }
    if trash_changed {
        merged.trashed_at = local.trashed_at;
    }
    merged.dirty = digest(&merged.content_enc) != digest(remote.content_enc.as_bytes())
        || merged.vault_id != remote.vault_id
        || merged.trashed_at != remote.trashed_at;
    merged
}

// ---- applying what the server sent ---------------------------------------

fn apply_remote_vault(
    store: &Store,
    session: &mut Session,
    remote: &SyncVault,
    now: i64,
) -> Result<bool, VaultError> {
    let local = store.vault(remote.id)?;
    match local {
        None => {
            let account = session.account.as_ref().ok_or(VaultError::Locked)?;
            // A vault this account cannot open is not kept: it would break
            // every list. It is logged by id only.
            let key = match vaults::unseal_key(
                &account.private,
                account.user_id,
                remote.id,
                remote.key_sealed.as_bytes(),
            ) {
                Ok(key) => key,
                Err(error) => {
                    tracing::warn!(vault = %remote.id, %error, "a vault from the server could not be opened");
                    return Ok(false);
                }
            };
            store.put_vault(&VaultRow {
                id: remote.id,
                key_enc: remote.key_sealed.as_bytes().to_vec(),
                meta_enc: remote.meta_enc.as_bytes().to_vec(),
                created_at: now,
                updated_at: now,
                server_revision: remote.revision,
                dirty: false,
            })?;
            session.vault_keys.insert(remote.id, key);
            Ok(true)
        }
        Some(mut row) => {
            if remote.revision <= row.server_revision {
                return Ok(false);
            }
            row.server_revision = remote.revision;
            if row.dirty && row.meta_enc != remote.meta_enc.as_bytes() {
                // Renamed here and elsewhere: this one is later.
            } else {
                row.meta_enc = remote.meta_enc.as_bytes().to_vec();
                row.dirty = false;
            }
            store.put_vault(&row)?;
            Ok(true)
        }
    }
}

/// Applies one item from the server. Returns what to do with the index.
fn apply_remote_item(store: &Store, remote: &SyncItem) -> Result<Option<Touch>, VaultError> {
    let local = store.item(remote.id)?;
    let tombstone: Option<Tombstone> = store
        .tombstones()?
        .into_iter()
        .find(|tombstone| tombstone.item_id == remote.id);

    if remote.deleted {
        // Deleted for good somewhere: deleted here too, whatever was changed
        // here in the meantime.
        store.remove_tombstone(remote.id)?;
        return Ok(match local {
            Some(_) => {
                store.delete_item(remote.id)?;
                Some(Touch::Remove(remote.id))
            }
            None => None,
        });
    }
    if let Some(mut tombstone) = tombstone {
        // Deleted for good here, and the server has a newer version of it.
        // The deletion is still waiting to be sent, against that version.
        if remote.revision > tombstone.server_revision {
            tombstone.server_revision = remote.revision;
            store.put_tombstone(&tombstone)?;
        }
        return Ok(None);
    }
    let Some(local) = local else {
        store.put_item(&adopt(remote, None))?;
        return Ok(Some(Touch::Upsert(remote.id)));
    };
    if remote.revision <= local.server_revision {
        // Already have it: the echo of our own write, or an old page.
        return Ok(None);
    }
    if !local.dirty {
        // Replacing a version with another: the old one stays in the history.
        if remote.version_id != local.version_id {
            store.add_revision(
                local.id,
                local.version_id,
                &local.content_enc,
                local.updated_at,
            )?;
        }
        store.put_item(&adopt(remote, Some(&local)))?;
        return Ok(Some(Touch::Upsert(remote.id)));
    }
    let merged = merge(&local, remote);
    // Whichever content lost is kept in the history.
    if merged.version_id != remote.version_id {
        store.add_revision(
            local.id,
            remote.version_id,
            remote.content_enc.as_bytes(),
            remote.updated_at,
        )?;
    } else if remote.version_id != local.version_id {
        store.add_revision(
            local.id,
            local.version_id,
            &local.content_enc,
            local.updated_at,
        )?;
    }
    store.put_item(&merged)?;
    Ok(Some(Touch::Upsert(remote.id)))
}

/// Brings the search index in line with the rows a pull changed.
fn refresh_index(
    store: &Store,
    session: &mut Session,
    touched: &[Touch],
) -> Result<(), VaultError> {
    for touch in touched {
        match touch {
            Touch::Remove(id) => session.index.remove(*id),
            Touch::Upsert(id) => {
                let content = items::row_and_key(store, session, *id).and_then(|(row, key)| {
                    let content = items::decrypt(&key, *id, &row.content_enc)?;
                    Ok((row, content))
                });
                match content {
                    Ok((row, content)) => session.index.upsert(items::entry(&row, &content)),
                    Err(error) => {
                        tracing::warn!(item = %id, %error, "an item from the server could not be read");
                        session.index.remove(*id);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Applies one page from the server and moves the cursor to its end.
pub fn apply_pull(
    store: &Store,
    session: &mut Session,
    page: &PullResponse,
    now: i64,
    report: &mut Report,
) -> Result<(), VaultError> {
    let mut touched = Vec::new();
    store.atomically(|store| {
        for vault in &page.vaults {
            if apply_remote_vault(store, session, vault, now).map_err(into_store)? {
                report.pulled += 1;
                report.changed = true;
            }
        }
        for item in &page.items {
            if let Some(touch) = apply_remote_item(store, item).map_err(into_store)? {
                touched.push(touch);
                report.pulled += 1;
                report.changed = true;
            }
        }
        store.set_cursor(page.cursor.max(store.cursor()?))
    })?;
    refresh_index(store, session, &touched)?;
    Ok(())
}

/// `atomically` wants a store error; anything else rolls the page back and
/// comes out as a corrupt-store error, which the next sync reports again.
fn into_store(error: VaultError) -> StoreError {
    match error {
        VaultError::Store(error) => error,
        VaultError::NotFound => StoreError::NotFound,
        other => {
            tracing::warn!(%other, "a page from the server could not be applied");
            StoreError::Corrupt
        }
    }
}

// ---- sending what is waiting -------------------------------------------------

/// What was sent, to compare with what the rows look like when the answer
/// comes back (the person may have edited in between).
struct Sent {
    vault_revision: Vec<(Uuid, Vec<u8>)>,
    items: Vec<(Uuid, Vec<u8>, Uuid, Option<i64>)>,
    tombstones: Vec<Uuid>,
}

/// A batch to send, and what was in it.
pub struct Batch {
    pub request: PushRequest,
    sent: Sent,
}

/// The kind a deletion carries. The server ignores it: it knows the item's
/// kind, and the local file does not keep one (it is inside the encrypted
/// content), so a deletion does not need to read the item.
const TOMBSTONE_KIND: ItemKind = ItemKind::Note;

/// Collects what is waiting to be sent, up to one batch. `skip` holds the
/// ids the server refused earlier in this sync.
pub fn prepare_push(
    store: &Store,
    session: &Session,
    skip: &HashSet<Uuid>,
) -> Result<Option<Batch>, VaultError> {
    let mut request = PushRequest::default();
    let mut sent = Sent {
        vault_revision: Vec::new(),
        items: Vec::new(),
        tombstones: Vec::new(),
    };
    for vault in store.vaults()? {
        if !vault.dirty || skip.contains(&vault.id) || request.vaults.len() >= MAX_BATCH {
            continue;
        }
        sent.vault_revision.push((vault.id, vault.meta_enc.clone()));
        request.vaults.push(SyncVault {
            id: vault.id,
            key_sealed: Opaque::new(vault.key_enc),
            meta_enc: Opaque::new(vault.meta_enc),
            revision: vault.server_revision,
        });
    }
    for item in store.items()? {
        if !item.dirty || skip.contains(&item.id) || request.items.len() >= MAX_BATCH {
            continue;
        }
        // The kind is read from the content, which only this unlocked
        // computer can do; it is sent in the clear because the server files
        // items by it (decision A6).
        let kind = match items::item_key(session, &item)
            .and_then(|key| items::decrypt(&key, item.id, &item.content_enc))
        {
            Ok(content) => content.kind(),
            Err(error) => {
                tracing::warn!(item = %item.id, %error, "an item could not be read, so it is not sent");
                continue;
            }
        };
        sent.items.push((
            item.id,
            digest(&item.content_enc),
            item.vault_id,
            item.trashed_at,
        ));
        request.items.push(SyncItem {
            id: item.id,
            vault_id: item.vault_id,
            kind,
            key_wrapped: Opaque::new(item.key_enc),
            content_enc: Opaque::new(item.content_enc),
            version_id: item.version_id,
            created_at: item.created_at,
            updated_at: item.updated_at,
            trashed_at: item.trashed_at,
            deleted: false,
            revision: item.server_revision,
        });
    }
    for tombstone in store.tombstones()? {
        if skip.contains(&tombstone.item_id) || request.items.len() >= MAX_BATCH {
            continue;
        }
        let kind = TOMBSTONE_KIND;
        sent.tombstones.push(tombstone.item_id);
        request.items.push(SyncItem {
            id: tombstone.item_id,
            vault_id: tombstone.vault_id,
            kind,
            key_wrapped: Opaque::default(),
            content_enc: Opaque::default(),
            version_id: Uuid::nil(),
            created_at: 0,
            updated_at: 0,
            trashed_at: None,
            deleted: true,
            revision: tombstone.server_revision,
        });
    }
    if request.vaults.is_empty() && request.items.is_empty() {
        return Ok(None);
    }
    Ok(Some(Batch { request, sent }))
}

/// Records the server's answers. Returns the ids it refused.
pub fn apply_push(
    store: &Store,
    session: &mut Session,
    batch: &Batch,
    response: &PushResponse,
    now: i64,
    report: &mut Report,
) -> Result<Vec<Uuid>, VaultError> {
    let mut refused = Vec::new();
    let mut touched = Vec::new();
    if response.vaults.len() != batch.request.vaults.len()
        || response.items.len() != batch.request.items.len()
    {
        return Err(VaultError::Store(StoreError::Corrupt));
    }
    store.atomically(|store| {
        for ((sent, (id, meta)), result) in batch
            .request
            .vaults
            .iter()
            .zip(&batch.sent.vault_revision)
            .zip(&response.vaults)
        {
            let _ = sent;
            match result {
                ChangeResult::Applied { revision } => {
                    if let Some(mut row) = store.vault(*id)? {
                        row.server_revision = *revision;
                        row.dirty = row.meta_enc != *meta;
                        store.put_vault(&row)?;
                        report.pushed += 1;
                    }
                }
                ChangeResult::Conflict { current } => {
                    apply_remote_vault(store, session, current, now).map_err(into_store)?;
                    report.conflicts += 1;
                    report.changed = true;
                }
                ChangeResult::Rejected => {
                    refused.push(*id);
                    report.rejected += 1;
                }
            }
        }
        let mut live = batch.sent.items.iter();
        for (change, result) in batch.request.items.iter().zip(&response.items) {
            if change.deleted {
                match result {
                    ChangeResult::Applied { .. } => {
                        store.remove_tombstone(change.id)?;
                        report.pushed += 1;
                    }
                    ChangeResult::Conflict { current } => {
                        // Either it is gone already, or it changed: the
                        // deletion is sent again on top of the new revision.
                        if let Some(touch) =
                            apply_remote_item(store, current).map_err(into_store)?
                        {
                            touched.push(touch);
                        }
                        report.conflicts += 1;
                    }
                    ChangeResult::Rejected => {
                        // Not ours, or never known to the server: nothing to delete.
                        store.remove_tombstone(change.id)?;
                    }
                }
                continue;
            }
            let Some((id, digest_sent, vault_sent, trashed_sent)) = live.next() else {
                return Err(StoreError::Corrupt);
            };
            match result {
                ChangeResult::Applied { revision } => {
                    if let Some(mut row) = store.item(*id)? {
                        row.server_revision = *revision;
                        row.base = Some(Base {
                            digest: digest_sent.clone(),
                            vault_id: *vault_sent,
                            trashed_at: *trashed_sent,
                        });
                        // Edited again while the request was on its way: still waiting.
                        row.dirty = digest(&row.content_enc) != *digest_sent
                            || row.vault_id != *vault_sent
                            || row.trashed_at != *trashed_sent;
                        store.put_item(&row)?;
                        report.pushed += 1;
                    }
                }
                ChangeResult::Conflict { current } => {
                    if let Some(touch) = apply_remote_item(store, current).map_err(into_store)? {
                        touched.push(touch);
                    }
                    report.conflicts += 1;
                    report.changed = true;
                }
                ChangeResult::Rejected => {
                    refused.push(*id);
                    report.rejected += 1;
                }
            }
        }
        Ok(())
    })?;
    refresh_index(store, session, &touched)?;
    Ok(refused)
}

/// One whole sync: pull, push, and again while there is something to settle.
pub async fn run<R: Remote, A: Access>(
    remote: &R,
    access: &A,
    now: impl Fn() -> i64 + Send + Sync,
) -> Result<Report, SyncError> {
    let mut report = Report::default();
    let mut refused: HashSet<Uuid> = HashSet::new();
    for _ in 0..MAX_ROUNDS {
        loop {
            let cursor = access.with(|store, _| Ok(store.cursor()?))?;
            let page = remote.pull(cursor, PULL_LIMIT).await?;
            let more = page.more;
            access.with(|store, session| apply_pull(store, session, &page, now(), &mut report))?;
            if !more {
                break;
            }
        }
        let Some(batch) = access.with(|store, session| prepare_push(store, session, &refused))?
        else {
            return Ok(report);
        };
        let response = remote.push(&batch.request).await?;
        let more_refused = access.with(|store, session| {
            apply_push(store, session, &batch, &response, now(), &mut report)
        })?;
        refused.extend(more_refused);
    }
    // Still changing at the same moment as another computer. The next sync
    // carries on from here.
    Ok(report)
}

/// Content that would be pushed is decryptable: a check used by tests.
#[doc(hidden)]
pub fn content_of(store: &Store, session: &Session, id: Uuid) -> Result<ItemContent, VaultError> {
    Ok(items::get(store, session, id)?.content)
}
