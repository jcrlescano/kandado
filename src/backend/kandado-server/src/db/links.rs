//! Secure links (D-32, D-33). The server keeps each link's encrypted payload
//! until the link stops working, then drops it at once: on the last allowed
//! view, on revoke, on expiry (the job), and when its item is deleted for good.
//!
//! Who may create a link is decided in the query, as for sync: the item must
//! be in a vault the caller is a member of with at least the editor role.

use kandado_protocol::{LinkInfo, LinkState, Opaque, OpenedLink};
use sqlx::{PgConnection, PgPool, Row, postgres::PgRow};
use time::OffsetDateTime;
use uuid::Uuid;

/// The most links one account may have working at the same time.
pub const MAX_ACTIVE: i64 = 500;

/// Ended links are listed (as expired, used up or revoked) this long, then deleted.
const KEEP_ENDED_DAYS: i32 = 30;

/// What [`create`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Created {
    Made {
        expires_at: OffsetDateTime,
    },
    /// The item does not exist, is deleted, or is not the caller's to share.
    NoItem,
    /// The id is already taken.
    Taken,
    /// The caller has [`MAX_ACTIVE`] working links already.
    TooMany,
}

pub struct NewLink<'a> {
    pub id: Uuid,
    pub item_id: Uuid,
    pub payload: &'a [u8],
    pub expires_in: i64,
    pub max_views: Option<i32>,
}

/// The condition for a link that still works. A macro, so queries stay literals.
macro_rules! live {
    () => {
        "payload_enc IS NOT NULL AND revoked_at IS NULL AND expires_at > now()
         AND (max_views IS NULL OR views < max_views)"
    };
}

pub async fn create(db: &PgPool, owner: Uuid, link: NewLink<'_>) -> sqlx::Result<Created> {
    let mut tx = db.begin().await?;
    // One creation at a time per account, so the limit cannot be raced past.
    sqlx::query("SELECT pg_advisory_xact_lock(2, hashtext($1))")
        .bind(owner.to_string())
        .execute(&mut *tx)
        .await?;
    let shareable: Option<bool> = sqlx::query_scalar(
        "SELECT true FROM items i
         JOIN vault_members m ON m.vault_id = i.vault_id AND m.user_id = $1
         JOIN vaults v ON v.id = i.vault_id AND v.deleted_at IS NULL
         WHERE i.id = $2 AND i.deleted_at IS NULL AND i.kind <> 'passkey'
           AND m.role IN ('editor', 'manager', 'owner')",
    )
    .bind(owner)
    .bind(link.item_id)
    .fetch_optional(&mut *tx)
    .await?;
    if shareable.is_none() {
        return Ok(Created::NoItem);
    }
    let active: i64 = sqlx::query_scalar(concat!(
        "SELECT count(*) FROM secure_links WHERE owner_id = $1 AND ",
        live!()
    ))
    .bind(owner)
    .fetch_one(&mut *tx)
    .await?;
    if active >= MAX_ACTIVE {
        return Ok(Created::TooMany);
    }
    let expires_at: Option<OffsetDateTime> = sqlx::query_scalar(
        "INSERT INTO secure_links (id, owner_id, item_id, payload_enc, expires_at, max_views)
         VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5), $6)
         ON CONFLICT (id) DO NOTHING
         RETURNING expires_at",
    )
    .bind(link.id)
    .bind(owner)
    .bind(link.item_id)
    .bind(link.payload)
    .bind(link.expires_in as f64)
    .bind(link.max_views)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(expires_at) = expires_at else {
        return Ok(Created::Taken);
    };
    tx.commit().await?;
    Ok(Created::Made { expires_at })
}

fn info_from(row: &PgRow) -> sqlx::Result<LinkInfo> {
    let state: String = row.try_get("state")?;
    Ok(LinkInfo {
        id: row.try_get("id")?,
        item_id: row.try_get("item_id")?,
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
        max_views: row.try_get("max_views")?,
        views: row.try_get("views")?,
        state: match state.as_str() {
            "revoked" => LinkState::Revoked,
            "used_up" => LinkState::UsedUp,
            "expired" => LinkState::Expired,
            _ => LinkState::Active,
        },
    })
}

/// The caller's links, newest first. Never the payloads.
pub async fn list(db: &PgPool, owner: Uuid) -> sqlx::Result<Vec<LinkInfo>> {
    let rows = sqlx::query(
        "SELECT id, item_id, created_at, expires_at, max_views, views,
                CASE
                    WHEN revoked_at IS NOT NULL THEN 'revoked'
                    WHEN max_views IS NOT NULL AND views >= max_views THEN 'used_up'
                    WHEN expires_at <= now() THEN 'expired'
                    ELSE 'active'
                END AS state
         FROM secure_links WHERE owner_id = $1
         ORDER BY created_at DESC, id DESC LIMIT 1000",
    )
    .bind(owner)
    .fetch_all(db)
    .await?;
    rows.iter().map(info_from).collect()
}

/// Stops a link at once. False if the caller has no link with that id.
/// Revoking a link that already ended is allowed and changes nothing more.
pub async fn revoke(db: &PgPool, owner: Uuid, id: Uuid) -> sqlx::Result<bool> {
    let found = sqlx::query(
        "UPDATE secure_links
         SET revoked_at = CASE
                 WHEN payload_enc IS NULL THEN revoked_at
                 ELSE COALESCE(revoked_at, now()) END,
             payload_enc = NULL
         WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(owner)
    .execute(db)
    .await?
    .rows_affected();
    Ok(found == 1)
}

/// Counts one view and hands out the payload, if the link still works. On the
/// last allowed view the payload is dropped in the same transaction.
pub async fn open(db: &PgPool, id: Uuid) -> sqlx::Result<Option<OpenedLink>> {
    let mut tx = db.begin().await?;
    let row = sqlx::query(concat!(
        "UPDATE secure_links SET views = views + 1 WHERE id = $1 AND ",
        live!(),
        " RETURNING payload_enc, expires_at, max_views, views"
    ))
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let payload: Vec<u8> = row.try_get("payload_enc")?;
    let max_views: Option<i32> = row.try_get("max_views")?;
    let views: i32 = row.try_get("views")?;
    let views_left = max_views.map(|max| (max - views).max(0));
    if views_left == Some(0) {
        sqlx::query("UPDATE secure_links SET payload_enc = NULL WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Some(OpenedLink {
        payload_enc: Opaque::new(payload),
        expires_at: row.try_get("expires_at")?,
        views_left,
    }))
}

/// Stops every link to an item. Called when the item is deleted for good.
pub async fn end_for_item(conn: &mut PgConnection, item_id: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE secure_links
         SET payload_enc = NULL, revoked_at = COALESCE(revoked_at, now())
         WHERE item_id = $1 AND payload_enc IS NOT NULL",
    )
    .bind(item_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// The link expiry job: drops the payloads of links that ended, and deletes
/// links that ended more than 30 days ago. Returns `(payloads dropped, links deleted)`.
pub async fn expire(db: &PgPool) -> sqlx::Result<(u64, u64)> {
    let dropped = sqlx::query(concat!(
        "UPDATE secure_links SET payload_enc = NULL WHERE payload_enc IS NOT NULL AND NOT (",
        live!(),
        ")"
    ))
    .execute(db)
    .await?
    .rows_affected();
    let deleted = sqlx::query(
        "DELETE FROM secure_links
         WHERE payload_enc IS NULL
           AND COALESCE(revoked_at, LEAST(expires_at, now())) < now() - make_interval(days => $1)",
    )
    .bind(KEEP_ENDED_DAYS)
    .execute(db)
    .await?
    .rows_affected();
    Ok((dropped, deleted))
}

/// Moves a link's dates back, for tests of expiry.
#[cfg(test)]
pub async fn age(db: &PgPool, id: Uuid, days: i32) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE secure_links
         SET created_at = created_at - make_interval(days => $2),
             expires_at = expires_at - make_interval(days => $2),
             revoked_at = revoked_at - make_interval(days => $2)
         WHERE id = $1",
    )
    .bind(id)
    .bind(days)
    .execute(db)
    .await?;
    Ok(())
}
