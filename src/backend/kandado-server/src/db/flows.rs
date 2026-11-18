//! Sign-in state between the browser leaving and coming back, and the
//! single-use codes that follow.

use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

/// A Google flow in progress.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Flow {
    pub nonce: String,
    pub pkce_verifier: String,
    pub client: String,
    pub redirect: String,
    pub client_challenge: String,
    pub purpose: String,
    pub user_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
}

/// Flows live ten minutes: long enough to type a password and a code at Google.
const FLOW_MINUTES: i32 = 10;
/// Codes live 60 seconds.
const HANDOFF_SECONDS: i32 = 60;

pub async fn save_flow(db: &PgPool, state_hash: &[u8], flow: &Flow) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO auth_flows (
            state_hash, nonce, pkce_verifier, client, redirect, client_challenge,
            purpose, user_id, device_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now() + make_interval(mins => $10))",
    )
    .bind(state_hash)
    .bind(&flow.nonce)
    .bind(&flow.pkce_verifier)
    .bind(&flow.client)
    .bind(&flow.redirect)
    .bind(&flow.client_challenge)
    .bind(&flow.purpose)
    .bind(flow.user_id)
    .bind(flow.device_id)
    .bind(FLOW_MINUTES)
    .execute(db)
    .await?;
    Ok(())
}

/// Takes the flow out of the table: a state can be used once.
pub async fn take_flow(db: &PgPool, state_hash: &[u8]) -> sqlx::Result<Option<Flow>> {
    sqlx::query_as(
        "DELETE FROM auth_flows WHERE state_hash = $1 AND expires_at > now()
         RETURNING nonce, pkce_verifier, client, redirect, client_challenge,
                   purpose, user_id, device_id",
    )
    .bind(state_hash)
    .fetch_optional(db)
    .await
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Handoff {
    pub challenge: String,
    pub user_id: Uuid,
    pub purpose: String,
    pub device_id: Option<Uuid>,
    pub auth_time: OffsetDateTime,
}

pub async fn save_handoff(
    db: &PgPool,
    code_hash: &[u8],
    challenge: &str,
    user_id: Uuid,
    purpose: &str,
    device_id: Option<Uuid>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO auth_handoffs (
            code_hash, challenge, user_id, purpose, device_id, auth_time, expires_at)
         VALUES ($1, $2, $3, $4, $5, now(), now() + make_interval(secs => $6))",
    )
    .bind(code_hash)
    .bind(challenge)
    .bind(user_id)
    .bind(purpose)
    .bind(device_id)
    .bind(HANDOFF_SECONDS)
    .execute(db)
    .await?;
    Ok(())
}

/// Takes the code out of the table: a code can be used once, even when the
/// verifier turns out to be wrong.
pub async fn take_handoff(db: &PgPool, code_hash: &[u8]) -> sqlx::Result<Option<Handoff>> {
    sqlx::query_as(
        "DELETE FROM auth_handoffs WHERE code_hash = $1 AND expires_at > now()
         RETURNING challenge, user_id, purpose, device_id, auth_time",
    )
    .bind(code_hash)
    .fetch_optional(db)
    .await
}

/// Housekeeping for the session-cleanup job (a later phase calls it daily).
#[allow(dead_code)]
pub async fn purge_expired(db: &PgPool) -> sqlx::Result<u64> {
    let flows = sqlx::query("DELETE FROM auth_flows WHERE expires_at <= now()")
        .execute(db)
        .await?;
    let handoffs = sqlx::query("DELETE FROM auth_handoffs WHERE expires_at <= now()")
        .execute(db)
        .await?;
    Ok(flows.rows_affected() + handoffs.rows_affected())
}
