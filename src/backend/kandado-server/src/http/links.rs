//! Secure links (D-32, D-33): create, list and revoke for the owner, and the
//! public route the viewer page calls to open one.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use kandado_protocol::{
    CreateLink, LINK_EXPIRY_CHOICES, LINK_VIEW_CHOICES, LinkCreated, LinkList, MAX_LINK_PAYLOAD,
    OpenedLink, ProblemCode,
};
use uuid::Uuid;

use super::AppState;
use crate::{
    auth::Session,
    db::links::{self, Created, NewLink},
    error::AppError,
};

/// `POST /v1/links`
pub async fn create(
    State(state): State<AppState>,
    session: Session,
    Json(request): Json<CreateLink>,
) -> Result<(StatusCode, Json<LinkCreated>), AppError> {
    if !LINK_EXPIRY_CHOICES.contains(&request.expires_in) {
        return Err(AppError::client(
            ProblemCode::BadRequest,
            "expires_in must be 1 hour, 1 day, 7 days or 30 days",
        ));
    }
    if !LINK_VIEW_CHOICES.contains(&request.max_views) {
        return Err(AppError::client(
            ProblemCode::BadRequest,
            "max_views must be 1, 5, 10 or none",
        ));
    }
    if request.payload_enc.is_empty() {
        return Err(AppError::client(
            ProblemCode::BadRequest,
            "the payload is empty",
        ));
    }
    if request.payload_enc.len() > MAX_LINK_PAYLOAD {
        return Err(AppError::client(
            ProblemCode::PayloadTooLarge,
            "the item is too large to share by link",
        ));
    }
    let created = links::create(
        &state.db,
        session.user_id,
        NewLink {
            id: request.id,
            item_id: request.item_id,
            payload: request.payload_enc.as_bytes(),
            expires_in: request.expires_in,
            max_views: request.max_views,
        },
    )
    .await?;
    match created {
        Created::Made { expires_at } => Ok((
            StatusCode::CREATED,
            Json(LinkCreated {
                url: format!("{}/l/{}", state.config.public_url, request.id),
                expires_at,
            }),
        )),
        Created::NoItem => Err(AppError::client(
            ProblemCode::NotFound,
            "no such item, or it cannot be shared",
        )),
        Created::Taken => Err(AppError::client(
            ProblemCode::Conflict,
            "a link with this id exists",
        )),
        Created::TooMany => Err(AppError::client(
            ProblemCode::RateLimited,
            "revoke some links before making more",
        )),
    }
}

/// `GET /v1/links`
pub async fn list(
    State(state): State<AppState>,
    session: Session,
) -> Result<Json<LinkList>, AppError> {
    Ok(Json(LinkList {
        links: links::list(&state.db, session.user_id).await?,
    }))
}

/// `DELETE /v1/links/{id}`
pub async fn revoke(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    if links::revoke(&state.db, session.user_id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::client(ProblemCode::NotFound, "no such link"))
    }
}

/// `POST /v1/public/links/{id}/open`. Public. POST, so that a crawler or a
/// link preview fetching the page does not use up a view.
pub async fn open(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<OpenedLink>, AppError> {
    links::open(&state.db, id)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::client(ProblemCode::NotFound, "this link does not work any more"))
}
