//! Signed-in devices (D-49).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use kandado_protocol::{DeviceList, ProblemCode};
use uuid::Uuid;

use super::AppState;
use crate::{
    auth::Session,
    db::{devices, users},
    error::AppError,
};

/// `GET /v1/devices`
pub async fn list(
    State(state): State<AppState>,
    session: Session,
) -> Result<Json<DeviceList>, AppError> {
    Ok(Json(DeviceList {
        devices: devices::list(&state.db, session.user_id, session.device_id).await?,
    }))
}

/// `DELETE /v1/devices/{id}`
pub async fn revoke(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    if !devices::revoke(&state.db, session.user_id, id).await? {
        return Err(AppError::client(ProblemCode::NotFound, "no such device"));
    }
    let mut conn = state.db.acquire().await?;
    users::audit(
        &mut conn,
        session.user_id,
        Some(session.device_id),
        "device_revoked",
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/devices/revoke-others`: needs a fresh Google sign-in.
pub async fn revoke_others(
    State(state): State<AppState>,
    session: Session,
) -> Result<StatusCode, AppError> {
    session.require_fresh()?;
    devices::revoke_others(&state.db, session.user_id, session.device_id).await?;
    let mut conn = state.db.acquire().await?;
    users::audit(
        &mut conn,
        session.user_id,
        Some(session.device_id),
        "others_revoked",
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
