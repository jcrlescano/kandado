//! Monitor routes (D-35 to D-39, E-15). Signed-in devices only, so the
//! server is not an open relay for anyone's lookups.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use kandado_protocol::{BreachList, ProblemCode, TwoFactorSites};

use super::AppState;
use crate::{
    auth::Session,
    error::AppError,
    monitor::{MonitorError, valid_prefix},
};

fn unavailable(error: MonitorError) -> AppError {
    tracing::warn!(%error, "a Monitor source failed");
    AppError::client(
        ProblemCode::Unavailable,
        "the breach and two-factor lists are not available right now",
    )
}

/// `GET /v1/monitor/pwned/{prefix}`: the padded range answer, as text.
pub async fn pwned(
    State(state): State<AppState>,
    _session: Session,
    Path(prefix): Path<String>,
) -> Result<Response, AppError> {
    if !valid_prefix(&prefix) {
        return Err(AppError::client(
            ProblemCode::BadRequest,
            "a prefix is five hex characters",
        ));
    }
    let answer = state.monitor.pwned(&prefix).await.map_err(unavailable)?;
    let mut response = (*answer).clone().into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    // Only this device's own cache, never a shared one.
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=86400"),
    );
    Ok(response)
}

/// `GET /v1/monitor/breaches`
pub async fn breaches(
    State(state): State<AppState>,
    _session: Session,
) -> Result<Json<BreachList>, AppError> {
    let list = state.monitor.breaches().await.map_err(unavailable)?;
    Ok(Json(Arc::unwrap_or_clone(list)))
}

/// `GET /v1/monitor/2fa-sites`
pub async fn two_factor_sites(
    State(state): State<AppState>,
    _session: Session,
) -> Result<Json<TwoFactorSites>, AppError> {
    let list = state.monitor.two_factor().await.map_err(unavailable)?;
    Ok(Json(Arc::unwrap_or_clone(list)))
}
