//! Sync and item history: `GET` and `POST /v1/sync`, and the revisions of an
//! item. The server orders and authorises; it never reads what it stores.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use kandado_protocol::{
    ItemVersion, ItemVersions, MAX_BATCH, ProblemCode, PullResponse, PushRequest, PushResponse,
};
use serde::Deserialize;
use uuid::Uuid;

use super::AppState;
use crate::{auth::Session, db::sync, error::AppError};

/// Rows in one pull when the device does not say.
const DEFAULT_LIMIT: i64 = 200;
const MAX_LIMIT: i64 = 500;

#[derive(Deserialize)]
pub struct PullQuery {
    #[serde(default)]
    cursor: i64,
    limit: Option<i64>,
}

/// `GET /v1/sync?cursor=N&limit=M`
pub async fn pull(
    State(state): State<AppState>,
    session: Session,
    Query(query): Query<PullQuery>,
) -> Result<Json<PullResponse>, AppError> {
    if query.cursor < 0 {
        return Err(AppError::client(
            ProblemCode::BadRequest,
            "cursor must not be negative",
        ));
    }
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    Ok(Json(
        sync::pull(&state.db, session.user_id, query.cursor, limit).await?,
    ))
}

/// `POST /v1/sync`
pub async fn push(
    State(state): State<AppState>,
    session: Session,
    Json(request): Json<PushRequest>,
) -> Result<Json<PushResponse>, AppError> {
    if request.vaults.len() > MAX_BATCH || request.items.len() > MAX_BATCH {
        return Err(AppError::client(
            ProblemCode::PayloadTooLarge,
            "send fewer changes at a time",
        ));
    }
    Ok(Json(sync::push(&state.db, session.user_id, request).await?))
}

/// `GET /v1/items/{id}/revisions`
pub async fn revisions(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<Uuid>,
) -> Result<Json<ItemVersions>, AppError> {
    sync::versions(&state.db, session.user_id, id)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::client(ProblemCode::NotFound, "no such item"))
}

/// `GET /v1/items/{id}/revisions/{version}`
pub async fn revision(
    State(state): State<AppState>,
    session: Session,
    Path((id, version)): Path<(Uuid, Uuid)>,
) -> Result<Json<ItemVersion>, AppError> {
    sync::version(&state.db, session.user_id, id, version)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::client(ProblemCode::NotFound, "no such version"))
}
