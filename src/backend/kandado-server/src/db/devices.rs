use kandado_protocol::Device;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

pub async fn create(
    db: &PgPool,
    user_id: Uuid,
    name: &str,
    platform: &str,
    refresh_hash: &[u8],
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "INSERT INTO devices (user_id, name, platform, refresh_token_hash)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(user_id)
    .bind(name)
    .bind(platform)
    .bind(refresh_hash)
    .fetch_one(db)
    .await
}

/// True while the device exists, belongs to the user and is not revoked.
/// Also keeps `last_seen_at` roughly current without a write on every call.
pub async fn touch_active(db: &PgPool, user_id: Uuid, device_id: Uuid) -> sqlx::Result<bool> {
    let active: Option<bool> =
        sqlx::query_scalar("SELECT revoked_at IS NULL FROM devices WHERE id = $1 AND user_id = $2")
            .bind(device_id)
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    if active != Some(true) {
        return Ok(false);
    }
    sqlx::query(
        "UPDATE devices SET last_seen_at = now()
         WHERE id = $1 AND last_seen_at < now() - interval '1 minute'",
    )
    .bind(device_id)
    .execute(db)
    .await?;
    Ok(true)
}

#[derive(Debug)]
pub enum Rotation {
    /// The token was current. The device now holds `new_hash`.
    Rotated {
        user_id: Uuid,
        device_id: Uuid,
        authenticated_at: OffsetDateTime,
    },
    /// The token was one that had already been rotated away: someone kept a
    /// copy. The device has been revoked.
    Reused,
    /// No such token, or the device is revoked.
    Unknown,
}

/// Swaps the refresh token of the device that holds `presented_hash`.
pub async fn rotate_refresh(
    db: &PgPool,
    presented_hash: &[u8],
    new_hash: &[u8],
) -> sqlx::Result<Rotation> {
    let mut tx = db.begin().await?;
    let current: Option<(Uuid, Uuid, OffsetDateTime)> = sqlx::query_as(
        "SELECT id, user_id, authenticated_at FROM devices
         WHERE refresh_token_hash = $1 AND revoked_at IS NULL FOR UPDATE",
    )
    .bind(presented_hash)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((device_id, user_id, authenticated_at)) = current {
        sqlx::query(
            "UPDATE devices SET
                previous_refresh_token_hash = refresh_token_hash,
                refresh_token_hash = $2,
                last_seen_at = now()
             WHERE id = $1",
        )
        .bind(device_id)
        .bind(new_hash)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(Rotation::Rotated {
            user_id,
            device_id,
            authenticated_at,
        });
    }

    let reused: Option<(Uuid, Uuid)> = sqlx::query_as(
        "UPDATE devices SET revoked_at = now()
         WHERE previous_refresh_token_hash = $1 AND revoked_at IS NULL
         RETURNING id, user_id",
    )
    .bind(presented_hash)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((device_id, user_id)) = reused {
        super::users::audit(&mut tx, user_id, Some(device_id), "refresh_token_reused").await?;
        tx.commit().await?;
        return Ok(Rotation::Reused);
    }
    Ok(Rotation::Unknown)
}

/// Records a fresh Google sign-in for the device, for step-up checks.
pub async fn mark_authenticated(
    db: &PgPool,
    device_id: Uuid,
    at: OffsetDateTime,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE devices SET authenticated_at = $2 WHERE id = $1")
        .bind(device_id)
        .bind(at)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn list(db: &PgPool, user_id: Uuid, current: Uuid) -> sqlx::Result<Vec<Device>> {
    let rows: Vec<(Uuid, String, String, OffsetDateTime, OffsetDateTime)> = sqlx::query_as(
        "SELECT id, name, platform, created_at, last_seen_at FROM devices
         WHERE user_id = $1 AND revoked_at IS NULL
         ORDER BY last_seen_at DESC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name, platform, created_at, last_seen_at)| Device {
            id,
            name,
            platform,
            created_at,
            last_seen_at,
            current: id == current,
        })
        .collect())
}

/// Revokes one device of the user. False when there is no such live device.
pub async fn revoke(db: &PgPool, user_id: Uuid, device_id: Uuid) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE devices SET revoked_at = now()
         WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(device_id)
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Revokes every device of the user except `keep`. Returns how many.
pub async fn revoke_others(db: &PgPool, user_id: Uuid, keep: Uuid) -> sqlx::Result<u64> {
    let result = sqlx::query(
        "UPDATE devices SET revoked_at = now()
         WHERE user_id = $1 AND id <> $2 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .bind(keep)
    .execute(db)
    .await?;
    Ok(result.rows_affected())
}
