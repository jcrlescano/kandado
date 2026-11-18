use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::auth::google::Identity;

pub const PROVIDER: &str = "google";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub plan: String,
}

/// Finds the user for a Google identity, creating the account on first
/// sign-in. Keeps the stored name and address in step with Google.
pub async fn find_or_create(db: &PgPool, identity: &Identity) -> sqlx::Result<User> {
    let mut tx = db.begin().await?;

    let existing: Option<User> = sqlx::query_as(
        "SELECT u.id, u.email, u.display_name, u.plan
         FROM oauth_identities i JOIN users u ON u.id = i.user_id
         WHERE i.provider = $1 AND i.subject = $2 AND u.deleted_at IS NULL",
    )
    .bind(PROVIDER)
    .bind(&identity.subject)
    .fetch_optional(&mut *tx)
    .await?;

    let user = match existing {
        Some(user) => {
            // The address is only replaced when nobody else holds it.
            let updated: User = sqlx::query_as(
                "UPDATE users SET
                    display_name = $2,
                    email = CASE WHEN NOT EXISTS (
                        SELECT 1 FROM users o
                        WHERE lower(o.email) = lower($3) AND o.id <> $1 AND o.deleted_at IS NULL
                    ) THEN $3 ELSE email END
                 WHERE id = $1
                 RETURNING id, email, display_name, plan",
            )
            .bind(user.id)
            .bind(&identity.name)
            .bind(&identity.email)
            .fetch_one(&mut *tx)
            .await?;
            updated
        }
        None => {
            let user: User = sqlx::query_as(
                "INSERT INTO users (email, display_name) VALUES ($1, $2)
                 RETURNING id, email, display_name, plan",
            )
            .bind(&identity.email)
            .bind(&identity.name)
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO oauth_identities (provider, subject, user_id) VALUES ($1, $2, $3)",
            )
            .bind(PROVIDER)
            .bind(&identity.subject)
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
            user
        }
    };
    tx.commit().await?;
    Ok(user)
}

pub async fn get(db: &PgPool, id: Uuid) -> sqlx::Result<Option<User>> {
    sqlx::query_as(
        "SELECT id, email, display_name, plan FROM users WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(db)
    .await
}

/// Removes the account and, by cascade, every row that belongs to it.
pub async fn delete(db: &PgPool, id: Uuid) -> sqlx::Result<bool> {
    let result = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn audit(
    conn: &mut PgConnection,
    user_id: Uuid,
    device_id: Option<Uuid>,
    kind: &str,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO audit_events (user_id, device_id, kind) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(device_id)
        .bind(kind)
        .execute(conn)
        .await?;
    Ok(())
}

#[cfg(test)]
pub async fn audit_kinds(db: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<String>> {
    use sqlx::Row;
    let rows = sqlx::query("SELECT kind FROM audit_events WHERE user_id = $1 ORDER BY id")
        .bind(user_id)
        .fetch_all(db)
        .await?;
    Ok(rows.iter().map(|row| row.get("kind")).collect())
}
