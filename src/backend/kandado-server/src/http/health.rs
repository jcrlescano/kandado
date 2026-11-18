use axum::{Json, extract::State};
use kandado_protocol::{Health, HealthStatus};

use super::AppState;
use crate::error::AppError;

fn ok() -> Json<Health> {
    Json(Health {
        status: HealthStatus::Ok,
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

/// The process is up. Used as the liveness check, so it touches nothing else.
pub async fn healthz() -> Json<Health> {
    ok()
}

/// The server can reach its database. Used to decide whether to send traffic.
pub async fn readyz(State(state): State<AppState>) -> Result<Json<Health>, AppError> {
    sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .map_err(|error| match error {
            // Any failure here means "not ready", whatever the cause.
            sqlx::Error::PoolTimedOut | sqlx::Error::Io(_) => error,
            other => sqlx::Error::Io(std::io::Error::other(other.to_string())),
        })?;
    Ok(ok())
}
