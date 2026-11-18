//! Vaults, items and their versions: what a device may pull, what it may push,
//! and how the server decides between two writers.
//!
//! Authorisation lives in the SQL. Every read joins through `vault_members`
//! for the calling user, and every write first checks the caller's role in
//! the vault it touches, so a row that is not the caller's is indistinguishable
//! from one that does not exist.
//!
//! Writes by one user are serialised with an advisory lock, and revisions are
//! drawn inside it. Two devices of the same account therefore never commit
//! revisions out of order, which is what makes "everything after the cursor"
//! a safe way to pull.

use kandado_protocol::{
    ChangeResult, ItemKind, ItemVersion, ItemVersions, MAX_BLOB, Opaque, PullResponse, PushRequest,
    PushResponse, SyncItem, SyncVault,
};
use sqlx::{PgConnection, PgPool, Row, postgres::PgRow};
use uuid::Uuid;

/// The Postgres notification channel the event hub listens on.
pub const CHANNEL: &str = "kandado_sync";

/// What a role may do. Phase 3 has owners only; the others are here so that
/// sharing (phase 6) changes who is a member, not these rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Role {
    Viewer,
    Editor,
    Manager,
    Owner,
}

impl Role {
    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "viewer" => Self::Viewer,
            "editor" => Self::Editor,
            "manager" => Self::Manager,
            "owner" => Self::Owner,
            _ => return None,
        })
    }
}

fn kind_of(row: &PgRow) -> sqlx::Result<ItemKind> {
    let kind: String = row.try_get("kind")?;
    ItemKind::parse(&kind).ok_or_else(|| sqlx::Error::Decode("unknown item kind".into()))
}

fn vault_from(row: &PgRow) -> sqlx::Result<SyncVault> {
    Ok(SyncVault {
        id: row.try_get("id")?,
        key_sealed: Opaque::new(row.try_get("vault_key_sealed")?),
        meta_enc: Opaque::new(row.try_get("meta_enc")?),
        revision: row.try_get("revision")?,
    })
}

fn item_from(row: &PgRow) -> sqlx::Result<SyncItem> {
    let deleted: Option<time::OffsetDateTime> = row.try_get("deleted_at")?;
    Ok(SyncItem {
        id: row.try_get("id")?,
        vault_id: row.try_get("vault_id")?,
        kind: kind_of(row)?,
        key_wrapped: Opaque::new(row.try_get("item_key_wrapped")?),
        content_enc: Opaque::new(row.try_get("content_enc")?),
        version_id: row.try_get("version_id")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        trashed_at: row.try_get("trashed_at")?,
        deleted: deleted.is_some(),
        revision: row.try_get("revision")?,
    })
}

macro_rules! vault_columns {
    () => {
        "v.id, m.vault_key_sealed, v.meta_enc, v.revision"
    };
}
macro_rules! item_columns {
    () => {
        "i.id, i.vault_id, i.kind, i.item_key_wrapped, i.content_enc, i.version_id, \
         i.created_at, i.updated_at, i.trashed_at, i.deleted_at, i.revision"
    };
}

/// Everything the user can see that changed after `cursor`, oldest first, at
/// most `limit` rows in all.
pub async fn pull(
    db: &PgPool,
    user_id: Uuid,
    cursor: i64,
    limit: i64,
) -> sqlx::Result<PullResponse> {
    let vaults = sqlx::query(concat!(
        "SELECT ",
        vault_columns!(),
        " FROM vaults v
         JOIN vault_members m ON m.vault_id = v.id AND m.user_id = $1
         WHERE v.revision > $2 AND v.deleted_at IS NULL
         ORDER BY v.revision LIMIT $3"
    ))
    .bind(user_id)
    .bind(cursor)
    .bind(limit + 1)
    .fetch_all(db)
    .await?;
    let items = sqlx::query(concat!(
        "SELECT ",
        item_columns!(),
        " FROM items i
         JOIN vault_members m ON m.vault_id = i.vault_id AND m.user_id = $1
         WHERE i.revision > $2
         ORDER BY i.revision LIMIT $3"
    ))
    .bind(user_id)
    .bind(cursor)
    .bind(limit + 1)
    .fetch_all(db)
    .await?;

    enum Row {
        Vault(SyncVault),
        Item(SyncItem),
    }
    let mut rows: Vec<(i64, Row)> = Vec::new();
    for row in &vaults {
        let vault = vault_from(row)?;
        rows.push((vault.revision, Row::Vault(vault)));
    }
    for row in &items {
        let item = item_from(row)?;
        rows.push((item.revision, Row::Item(item)));
    }
    rows.sort_by_key(|(revision, _)| *revision);
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let more = rows.len() > limit;
    rows.truncate(limit);

    let next = rows.last().map_or(cursor, |(revision, _)| *revision);
    let (mut vaults, mut items) = (Vec::new(), Vec::new());
    for (_, row) in rows {
        match row {
            Row::Vault(vault) => vaults.push(vault),
            Row::Item(item) => items.push(item),
        }
    }
    Ok(PullResponse {
        vaults,
        items,
        cursor: next,
        more,
    })
}

/// The newest revision among everything the user can see.
pub async fn newest(db: &PgPool, user_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "SELECT GREATEST(
            COALESCE((SELECT MAX(v.revision) FROM vaults v
                      JOIN vault_members m ON m.vault_id = v.id AND m.user_id = $1), 0),
            COALESCE((SELECT MAX(i.revision) FROM items i
                      JOIN vault_members m ON m.vault_id = i.vault_id AND m.user_id = $1), 0))",
    )
    .bind(user_id)
    .fetch_one(db)
    .await
}

/// Takes the per-user lock that orders one account's writes. Held until the
/// transaction ends.
async fn lock_user(conn: &mut PgConnection, user_id: Uuid) -> sqlx::Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(1, hashtext($1))")
        .bind(user_id.to_string())
        .execute(conn)
        .await?;
    Ok(())
}

async fn next_revision(conn: &mut PgConnection) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT nextval('sync_revision')")
        .fetch_one(conn)
        .await
}

/// The caller's role in a vault, if they are a member.
async fn role_in(
    conn: &mut PgConnection,
    user_id: Uuid,
    vault_id: Uuid,
) -> sqlx::Result<Option<Role>> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT m.role FROM vault_members m JOIN vaults v ON v.id = m.vault_id
         WHERE m.vault_id = $1 AND m.user_id = $2 AND v.deleted_at IS NULL",
    )
    .bind(vault_id)
    .bind(user_id)
    .fetch_optional(conn)
    .await?;
    Ok(role.as_deref().and_then(Role::parse))
}

/// Applies a batch of changes from one device, all or nothing. Each change is
/// answered on its own: applied, in conflict, or rejected.
pub async fn push(db: &PgPool, user_id: Uuid, request: PushRequest) -> sqlx::Result<PushResponse> {
    let mut tx = db.begin().await?;
    lock_user(&mut tx, user_id).await?;
    let mut changed = false;

    let mut vaults = Vec::with_capacity(request.vaults.len());
    for change in &request.vaults {
        let result = push_vault(&mut tx, user_id, change).await?;
        changed |= matches!(result, ChangeResult::Applied { .. });
        vaults.push(result);
    }
    let mut items = Vec::with_capacity(request.items.len());
    for change in &request.items {
        let result = push_item(&mut tx, user_id, change).await?;
        changed |= matches!(result, ChangeResult::Applied { .. });
        items.push(result);
    }
    if changed {
        // Delivered when the transaction commits, never before.
        sqlx::query("SELECT pg_notify($1, $2)")
            .bind(CHANNEL)
            .bind(user_id.to_string())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(PushResponse {
        vaults,
        items,
        cursor: newest(db, user_id).await?,
    })
}

async fn push_vault(
    conn: &mut PgConnection,
    user_id: Uuid,
    change: &SyncVault,
) -> sqlx::Result<ChangeResult<SyncVault>> {
    if change.meta_enc.len() > MAX_BLOB || change.key_sealed.len() > MAX_BLOB {
        return Ok(ChangeResult::Rejected);
    }
    let exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM vaults WHERE id = $1 FOR UPDATE")
        .bind(change.id)
        .fetch_optional(&mut *conn)
        .await?;
    if exists.is_none() {
        if change.revision != 0 {
            return Ok(ChangeResult::Rejected);
        }
        let revision = next_revision(conn).await?;
        sqlx::query(
            "INSERT INTO vaults (id, owner_id, meta_enc, revision) VALUES ($1, $2, $3, $4)",
        )
        .bind(change.id)
        .bind(user_id)
        .bind(change.meta_enc.as_bytes())
        .bind(revision)
        .execute(&mut *conn)
        .await?;
        sqlx::query(
            "INSERT INTO vault_members (vault_id, user_id, role, vault_key_sealed)
             VALUES ($1, $2, 'owner', $3)",
        )
        .bind(change.id)
        .bind(user_id)
        .bind(change.key_sealed.as_bytes())
        .execute(&mut *conn)
        .await?;
        return Ok(ChangeResult::Applied { revision });
    }
    // Renaming a vault takes a manager or the owner.
    match role_in(conn, user_id, change.id).await? {
        Some(role) if role >= Role::Manager => {}
        _ => return Ok(ChangeResult::Rejected),
    }
    let current = sqlx::query(concat!(
        "SELECT ",
        vault_columns!(),
        " FROM vaults v
         JOIN vault_members m ON m.vault_id = v.id AND m.user_id = $1 WHERE v.id = $2"
    ))
    .bind(user_id)
    .bind(change.id)
    .fetch_one(&mut *conn)
    .await?;
    let current = vault_from(&current)?;
    if current.revision != change.revision {
        return Ok(ChangeResult::Conflict { current });
    }
    let revision = next_revision(conn).await?;
    sqlx::query("UPDATE vaults SET meta_enc = $2, revision = $3 WHERE id = $1")
        .bind(change.id)
        .bind(change.meta_enc.as_bytes())
        .bind(revision)
        .execute(&mut *conn)
        .await?;
    Ok(ChangeResult::Applied { revision })
}

async fn push_item(
    conn: &mut PgConnection,
    user_id: Uuid,
    change: &SyncItem,
) -> sqlx::Result<ChangeResult<SyncItem>> {
    if change.content_enc.len() > MAX_BLOB || change.key_wrapped.len() > MAX_BLOB {
        return Ok(ChangeResult::Rejected);
    }
    // Writing needs at least the editor role in the vault the item goes to.
    match role_in(conn, user_id, change.vault_id).await? {
        Some(role) if role >= Role::Editor => {}
        _ => return Ok(ChangeResult::Rejected),
    }
    let current = sqlx::query(concat!(
        "SELECT ",
        item_columns!(),
        " FROM items i WHERE i.id = $1 FOR UPDATE"
    ))
    .bind(change.id)
    .fetch_optional(&mut *conn)
    .await?;

    let Some(current) = current else {
        if change.revision != 0 {
            return Ok(ChangeResult::Rejected);
        }
        let revision = next_revision(conn).await?;
        insert_item(conn, user_id, change, revision).await?;
        return Ok(ChangeResult::Applied { revision });
    };
    let current_item = item_from(&current)?;
    // The item must be one the caller can already see, and may not change kind.
    match role_in(conn, user_id, current_item.vault_id).await? {
        Some(role) if role >= Role::Editor => {}
        _ => return Ok(ChangeResult::Rejected),
    }
    // A deletion carries no meaningful kind: the clients do not keep one.
    if !change.deleted && current_item.kind != change.kind {
        return Ok(ChangeResult::Rejected);
    }
    // A tombstone wins over any edit made before the device heard of it, and
    // so does a newer revision: the device merges and sends again.
    if current_item.deleted || current_item.revision != change.revision {
        return Ok(ChangeResult::Conflict {
            current: current_item,
        });
    }
    let revision = next_revision(conn).await?;
    update_item(conn, user_id, change, &current_item, revision).await?;
    Ok(ChangeResult::Applied { revision })
}

async fn insert_item(
    conn: &mut PgConnection,
    user_id: Uuid,
    change: &SyncItem,
    revision: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO items (id, vault_id, kind, item_key_wrapped, content_enc, version_id,
                            revision, created_at, updated_at, trashed_at, trashed_on, deleted_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                 CASE WHEN $10::bigint IS NULL THEN NULL ELSE now() END,
                 CASE WHEN $11 THEN now() END)",
    )
    .bind(change.id)
    .bind(change.vault_id)
    .bind(change.kind.as_str())
    .bind(change.key_wrapped.as_bytes())
    .bind(change.content_enc.as_bytes())
    .bind(change.version_id)
    .bind(revision)
    .bind(change.created_at)
    .bind(change.updated_at)
    .bind(change.trashed_at)
    .bind(change.deleted)
    .execute(&mut *conn)
    .await?;
    if !change.deleted {
        keep_version(conn, user_id, change).await?;
    }
    Ok(())
}

async fn update_item(
    conn: &mut PgConnection,
    user_id: Uuid,
    change: &SyncItem,
    current: &SyncItem,
    revision: i64,
) -> sqlx::Result<()> {
    if change.deleted {
        // Deleted for good: only the marker stays.
        sqlx::query(
            "UPDATE items SET content_enc = '\\x', item_key_wrapped = '\\x', revision = $2,
                    updated_at = $3, deleted_at = now(), trashed_on = NULL
             WHERE id = $1",
        )
        .bind(change.id)
        .bind(revision)
        .bind(change.updated_at)
        .execute(&mut *conn)
        .await?;
        sqlx::query("DELETE FROM item_revisions WHERE item_id = $1")
            .bind(change.id)
            .execute(&mut *conn)
            .await?;
        return Ok(());
    }
    // `trashed_on` is the server's clock: set when the item first arrives in
    // the trash, cleared when it leaves, untouched while it stays.
    sqlx::query(
        "UPDATE items SET vault_id = $2, item_key_wrapped = $3, content_enc = $4,
                version_id = $5, revision = $6, updated_at = $7, trashed_at = $8,
                trashed_on = CASE
                    WHEN $8::bigint IS NULL THEN NULL
                    ELSE COALESCE(trashed_on, now()) END
         WHERE id = $1",
    )
    .bind(change.id)
    .bind(change.vault_id)
    .bind(change.key_wrapped.as_bytes())
    .bind(change.content_enc.as_bytes())
    .bind(change.version_id)
    .bind(revision)
    .bind(change.updated_at)
    .bind(change.trashed_at)
    .execute(&mut *conn)
    .await?;
    if change.version_id != current.version_id {
        keep_version(conn, user_id, change).await?;
    }
    Ok(())
}

/// Records the content of a change as a version of its item.
async fn keep_version(
    conn: &mut PgConnection,
    user_id: Uuid,
    change: &SyncItem,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO item_revisions (item_id, version_id, content_enc, item_key_wrapped, author_id, saved_at)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (item_id, version_id) DO NOTHING",
    )
    .bind(change.id)
    .bind(change.version_id)
    .bind(change.content_enc.as_bytes())
    .bind(change.key_wrapped.as_bytes())
    .bind(user_id)
    .bind(change.updated_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Every saved version of an item, newest first. `None` if the caller cannot
/// see the item.
pub async fn versions(
    db: &PgPool,
    user_id: Uuid,
    item_id: Uuid,
) -> sqlx::Result<Option<ItemVersions>> {
    let visible: Option<bool> = sqlx::query_scalar(
        "SELECT i.deleted_at IS NULL FROM items i
         JOIN vault_members m ON m.vault_id = i.vault_id AND m.user_id = $1
         WHERE i.id = $2",
    )
    .bind(user_id)
    .bind(item_id)
    .fetch_optional(db)
    .await?;
    if visible != Some(true) {
        return Ok(None);
    }
    let rows = sqlx::query(
        "SELECT version_id, content_enc, item_key_wrapped, saved_at FROM item_revisions
         WHERE item_id = $1 ORDER BY saved_at DESC, created_at DESC",
    )
    .bind(item_id)
    .fetch_all(db)
    .await?;
    let versions = rows
        .iter()
        .map(|row| {
            Ok(ItemVersion {
                version_id: row.try_get("version_id")?,
                content_enc: Opaque::new(row.try_get("content_enc")?),
                key_wrapped: Opaque::new(row.try_get("item_key_wrapped")?),
                saved_at: row.try_get("saved_at")?,
            })
        })
        .collect::<sqlx::Result<_>>()?;
    Ok(Some(ItemVersions { versions }))
}

/// One version of an item.
pub async fn version(
    db: &PgPool,
    user_id: Uuid,
    item_id: Uuid,
    version_id: Uuid,
) -> sqlx::Result<Option<ItemVersion>> {
    Ok(versions(db, user_id, item_id).await?.and_then(|all| {
        all.versions
            .into_iter()
            .find(|version| version.version_id == version_id)
    }))
}

/// Deletes, for good, every item that has been in the trash longer than
/// `older_than_days` days (30 in production), leaving tombstones. Returns how
/// many. One transaction per user, under that user's lock, so the new
/// revisions stay in order.
pub async fn purge_trash(db: &PgPool, older_than_days: i32) -> sqlx::Result<u64> {
    let users: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT m.user_id FROM items i
         JOIN vault_members m ON m.vault_id = i.vault_id
         WHERE i.deleted_at IS NULL AND i.trashed_on IS NOT NULL
           AND i.trashed_on < now() - make_interval(days => $1)",
    )
    .bind(older_than_days)
    .fetch_all(db)
    .await?;
    let mut purged = 0;
    for user_id in users {
        let mut tx = db.begin().await?;
        lock_user(&mut tx, user_id).await?;
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT i.id FROM items i
             JOIN vault_members m ON m.vault_id = i.vault_id AND m.user_id = $1
             WHERE i.deleted_at IS NULL AND i.trashed_on IS NOT NULL
               AND i.trashed_on < now() - make_interval(days => $2)
             FOR UPDATE OF i",
        )
        .bind(user_id)
        .bind(older_than_days)
        .fetch_all(&mut *tx)
        .await?;
        for id in &ids {
            let revision = next_revision(&mut tx).await?;
            sqlx::query(
                "UPDATE items SET content_enc = '\\x', item_key_wrapped = '\\x', revision = $2,
                        deleted_at = now(), trashed_on = NULL WHERE id = $1",
            )
            .bind(id)
            .bind(revision)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM item_revisions WHERE item_id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        if !ids.is_empty() {
            sqlx::query("SELECT pg_notify($1, $2)")
                .bind(CHANNEL)
                .bind(user_id.to_string())
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        purged += ids.len() as u64;
    }
    Ok(purged)
}
