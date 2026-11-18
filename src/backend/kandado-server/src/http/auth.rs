//! Sign-in: the Google flow, the code-for-token exchange, refresh and logout.
//! See `docs/architecture/security.md`.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::Redirect,
};
use kandado_protocol::{
    ClientKind, FlowPurpose, ProblemCode, RefreshRequest, StartQuery, TokenRequest, TokenResponse,
};
use serde::Deserialize;
use time::OffsetDateTime;
use url::Url;
use uuid::Uuid;

use super::AppState;
use crate::{
    auth::{
        Session,
        google::GoogleError,
        redirect_allowed,
        token::{self, ACCESS_TOKEN_SECONDS},
    },
    db::{
        devices::{self, Rotation},
        flows::{self, Flow},
        users,
    },
    error::AppError,
};

fn bad_request(detail: &str) -> AppError {
    AppError::client(ProblemCode::BadRequest, detail)
}

fn internal(error: impl std::fmt::Display) -> AppError {
    AppError::Internal(error.to_string())
}

fn purpose_name(purpose: FlowPurpose) -> &'static str {
    match purpose {
        FlowPurpose::SignIn => "sign_in",
        FlowPurpose::StepUp => "step_up",
    }
}

fn client_name(client: ClientKind) -> &'static str {
    match client {
        ClientKind::Desktop => "desktop",
        ClientKind::Extension => "extension",
    }
}

/// A SHA-256 challenge is 32 bytes, so 43 characters of unpadded base64url.
fn valid_challenge(challenge: &str) -> bool {
    challenge.len() == 43
        && challenge
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Checks a start request and, for a step-up, finds the session it confirms.
async fn validate_start(
    state: &AppState,
    query: &StartQuery,
) -> Result<(Option<Uuid>, Option<Uuid>), AppError> {
    if !redirect_allowed(
        query.client,
        &query.redirect,
        &state.config.extension_redirect_hosts,
    ) {
        return Err(bad_request("redirect is not allowed"));
    }
    if !valid_challenge(&query.challenge) {
        return Err(bad_request("challenge must be a base64url SHA-256"));
    }
    match query.purpose {
        FlowPurpose::SignIn => Ok((None, None)),
        FlowPurpose::StepUp => {
            let unauthorized = || AppError::client(ProblemCode::Unauthorized, "sign in required");
            let claims = query
                .access_token
                .as_deref()
                .and_then(|value| state.signer.verify(value, OffsetDateTime::now_utc()))
                .ok_or_else(unauthorized)?;
            if !devices::touch_active(&state.db, claims.user_id, claims.device_id).await? {
                return Err(unauthorized());
            }
            Ok((Some(claims.user_id), Some(claims.device_id)))
        }
    }
}

fn google_unavailable(error: GoogleError) -> AppError {
    match error {
        GoogleError::NotConfigured | GoogleError::Unreachable(_) => {
            tracing::warn!(%error, "google sign-in unavailable");
            AppError::client(ProblemCode::Unavailable, "Google sign-in is not available")
        }
        other => internal(other),
    }
}

/// `GET /v1/auth/google/start`
pub async fn start(
    State(state): State<AppState>,
    Query(query): Query<StartQuery>,
) -> Result<Redirect, AppError> {
    let (user_id, device_id) = validate_start(&state, &query).await?;
    let flow_state = token::random_secret().map_err(internal)?;
    let authorization = state
        .google
        .authorize(&flow_state, query.purpose == FlowPurpose::StepUp)
        .await
        .map_err(google_unavailable)?;
    flows::save_flow(
        &state.db,
        &token::hash_secret(&flow_state),
        &Flow {
            nonce: authorization.nonce,
            pkce_verifier: authorization.pkce_verifier,
            client: client_name(query.client).to_owned(),
            redirect: query.redirect,
            client_challenge: query.challenge,
            purpose: purpose_name(query.purpose).to_owned(),
            user_id,
            device_id,
        },
    )
    .await?;
    Ok(Redirect::to(authorization.url.as_str()))
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
}

/// Sends the browser back to the client with a result in the query.
fn back_to_client(redirect: &str, key: &str, value: &str) -> Result<Redirect, AppError> {
    let mut url = Url::parse(redirect).map_err(internal)?;
    url.query_pairs_mut().append_pair(key, value);
    Ok(Redirect::to(url.as_str()))
}

/// `GET /v1/auth/google/callback`
pub async fn callback(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
) -> Result<Redirect, AppError> {
    let flow_state = query.state.ok_or_else(|| bad_request("state is missing"))?;
    let flow = flows::take_flow(&state.db, &token::hash_secret(&flow_state))
        .await?
        .ok_or_else(|| bad_request("this sign-in has expired; start again"))?;

    let Some(code) = query.code.filter(|_| query.error.is_none()) else {
        return back_to_client(&flow.redirect, "error", "access_denied");
    };
    let identity = match state
        .google
        .identify(&code, &flow.pkce_verifier, &flow.nonce)
        .await
    {
        Ok(identity) => identity,
        Err(error) => {
            tracing::warn!(%error, "google sign-in failed");
            return back_to_client(&flow.redirect, "error", "sign_in_failed");
        }
    };
    finish_flow(&state, &flow, &identity).await
}

/// Everything after Google (or the development stand-in) has said who the
/// person is: find or create the account, then hand a code to the client.
pub async fn finish_flow(
    state: &AppState,
    flow: &Flow,
    identity: &crate::auth::google::Identity,
) -> Result<Redirect, AppError> {
    let user = users::find_or_create(&state.db, identity).await?;
    // A step-up must be completed by the account it was started for.
    if flow.user_id.is_some_and(|expected| expected != user.id) {
        return back_to_client(&flow.redirect, "error", "wrong_account");
    }
    let code = token::random_secret().map_err(internal)?;
    flows::save_handoff(
        &state.db,
        &token::hash_secret(&code),
        &flow.client_challenge,
        user.id,
        &flow.purpose,
        flow.device_id,
    )
    .await?;
    back_to_client(&flow.redirect, "code", &code)
}

fn token_response(
    state: &AppState,
    user_id: Uuid,
    device_id: Uuid,
    auth_time: OffsetDateTime,
    refresh_token: Option<String>,
) -> TokenResponse {
    TokenResponse {
        access_token: state
            .signer
            .issue(user_id, device_id, auth_time, OffsetDateTime::now_utc()),
        expires_in: ACCESS_TOKEN_SECONDS,
        refresh_token,
        user_id,
        device_id,
    }
}

/// `POST /v1/auth/token`
pub async fn exchange(
    State(state): State<AppState>,
    Json(request): Json<TokenRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    let invalid = || bad_request("the sign-in code is invalid or has expired");
    // Taking the code deletes it, so a wrong verifier burns it too.
    let handoff = flows::take_handoff(&state.db, &token::hash_secret(&request.code))
        .await?
        .ok_or_else(invalid)?;
    if !token::challenge_matches(&request.verifier, &handoff.challenge) {
        return Err(invalid());
    }

    if handoff.purpose == purpose_name(FlowPurpose::StepUp) {
        let device_id = handoff.device_id.ok_or_else(invalid)?;
        if !devices::touch_active(&state.db, handoff.user_id, device_id).await? {
            return Err(invalid());
        }
        devices::mark_authenticated(&state.db, device_id, handoff.auth_time).await?;
        let mut conn = state.db.acquire().await?;
        users::audit(&mut conn, handoff.user_id, Some(device_id), "step_up").await?;
        return Ok(Json(token_response(
            &state,
            handoff.user_id,
            device_id,
            handoff.auth_time,
            None,
        )));
    }

    let device = request
        .device
        .ok_or_else(|| bad_request("device is required"))?;
    let (name, platform) = (device.name.trim(), device.platform.trim());
    if name.is_empty() || name.chars().count() > 100 || platform.chars().count() > 100 {
        return Err(bad_request("device name or platform is not valid"));
    }
    let refresh = token::random_secret().map_err(internal)?;
    let device_id = devices::create(
        &state.db,
        handoff.user_id,
        name,
        platform,
        &token::hash_secret(&refresh),
    )
    .await?;
    let mut conn = state.db.acquire().await?;
    users::audit(&mut conn, handoff.user_id, Some(device_id), "sign_in").await?;
    Ok(Json(token_response(
        &state,
        handoff.user_id,
        device_id,
        handoff.auth_time,
        Some(refresh),
    )))
}

/// `POST /v1/auth/refresh`
pub async fn refresh(
    State(state): State<AppState>,
    Json(request): Json<RefreshRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    let next = token::random_secret().map_err(internal)?;
    let rotation = devices::rotate_refresh(
        &state.db,
        &token::hash_secret(&request.refresh_token),
        &token::hash_secret(&next),
    )
    .await?;
    match rotation {
        Rotation::Rotated {
            user_id,
            device_id,
            authenticated_at,
        } => Ok(Json(token_response(
            &state,
            user_id,
            device_id,
            authenticated_at,
            Some(next),
        ))),
        Rotation::Reused => Err(AppError::client(
            ProblemCode::Unauthorized,
            "this session was signed out because its token was used twice",
        )),
        Rotation::Unknown => Err(AppError::client(
            ProblemCode::Unauthorized,
            "sign in required",
        )),
    }
}

/// `POST /v1/auth/logout`: signs out the calling device.
pub async fn logout(
    State(state): State<AppState>,
    session: Session,
) -> Result<StatusCode, AppError> {
    if devices::revoke(&state.db, session.user_id, session.device_id).await? {
        let mut conn = state.db.acquire().await?;
        users::audit(
            &mut conn,
            session.user_id,
            Some(session.device_id),
            "sign_out",
        )
        .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// The development sign-in: stands in for the Google leg so integration tests
/// and local work need no Google account. Compiled out of release builds, and
/// off unless `KANDADO_DEV_AUTH=1`.
#[cfg(debug_assertions)]
#[derive(Deserialize)]
pub struct DevSignInQuery {
    email: String,
    #[serde(flatten)]
    start: StartQuery,
}

#[cfg(debug_assertions)]
pub async fn dev_sign_in(
    State(state): State<AppState>,
    Query(query): Query<DevSignInQuery>,
) -> Result<Redirect, AppError> {
    if !state.config.dev_auth {
        return Err(AppError::client(ProblemCode::NotFound, "not found"));
    }
    let (user_id, device_id) = validate_start(&state, &query.start).await?;
    let flow = Flow {
        nonce: String::new(),
        pkce_verifier: String::new(),
        client: client_name(query.start.client).to_owned(),
        redirect: query.start.redirect,
        client_challenge: query.start.challenge,
        purpose: purpose_name(query.start.purpose).to_owned(),
        user_id,
        device_id,
    };
    let identity = crate::auth::google::Identity {
        subject: format!("dev:{}", query.email),
        name: query.email.split('@').next().unwrap_or_default().to_owned(),
        email: query.email,
    };
    finish_flow(&state, &flow, &identity).await
}
