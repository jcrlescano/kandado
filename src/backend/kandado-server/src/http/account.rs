//! The account and the wrapped keys the server keeps for it.

use axum::{Json, extract::State, http::StatusCode};
use kandado_protocol::{AccountKeys, ChangeMaster, KdfSettings, Me, ProblemCode};

use super::AppState;
use crate::{
    auth::Session,
    db::{keys, users},
    error::AppError,
};

// The server never decrypts, but it refuses blobs that cannot be valid, so a
// buggy client cannot store keys nobody can open. These mirror the formats in
// `kandado-crypto`; a test there-in checks that they agree.
const KEY_FORMAT_VERSION: u8 = 1;
const SALT_LEN: usize = 16;
/// A wrapped 32-byte key: version byte, 24-byte nonce, key, 16-byte tag.
const WRAPPED_KEY_LEN: usize = 1 + 24 + 32 + 16;
const PUBLIC_KEY_LEN: usize = 32;
const KDF_MEMORY_KIB: std::ops::RangeInclusive<u32> = (19 * 1024)..=(1024 * 1024);
const KDF_ITERATIONS: std::ops::RangeInclusive<u32> = 2..=16;
const KDF_PARALLELISM: std::ops::RangeInclusive<u32> = 1..=8;

fn invalid(detail: &str) -> AppError {
    AppError::client(ProblemCode::BadRequest, detail)
}

fn check_kdf(kdf: &KdfSettings, salt_len: usize) -> Result<(), AppError> {
    if salt_len != SALT_LEN {
        return Err(invalid("kdf_salt must be 16 bytes"));
    }
    if !KDF_MEMORY_KIB.contains(&kdf.memory_kib)
        || !KDF_ITERATIONS.contains(&kdf.iterations)
        || !KDF_PARALLELISM.contains(&kdf.parallelism)
    {
        return Err(invalid(
            "key derivation settings are outside the allowed range",
        ));
    }
    Ok(())
}

fn check_keys(keys: &AccountKeys) -> Result<(), AppError> {
    if keys.version != KEY_FORMAT_VERSION {
        return Err(invalid("unsupported key format version"));
    }
    check_kdf(&keys.kdf, keys.kdf_salt.len())?;
    if keys.user_key_by_master.len() != WRAPPED_KEY_LEN
        || keys.user_key_by_recovery.len() != WRAPPED_KEY_LEN
        || keys.private_keys_enc.len() != WRAPPED_KEY_LEN
        || keys.public_x25519.len() != PUBLIC_KEY_LEN
    {
        return Err(invalid("a wrapped key has the wrong length"));
    }
    Ok(())
}

/// `GET /v1/me`
pub async fn me(State(state): State<AppState>, session: Session) -> Result<Json<Me>, AppError> {
    let user = users::get(&state.db, session.user_id)
        .await?
        .ok_or_else(|| AppError::client(ProblemCode::Unauthorized, "sign in required"))?;
    Ok(Json(Me {
        id: user.id,
        email: user.email,
        display_name: user.display_name,
        plan: user.plan,
        has_keys: keys::exists(&state.db, user.id).await?,
        device_id: session.device_id,
    }))
}

/// `POST /v1/account/keys`: stores the keys created at D-02. Only once.
pub async fn create_keys(
    State(state): State<AppState>,
    session: Session,
    Json(body): Json<AccountKeys>,
) -> Result<StatusCode, AppError> {
    check_keys(&body)?;
    if !keys::insert(&state.db, session.user_id, &body).await? {
        return Err(AppError::client(
            ProblemCode::Conflict,
            "this account already has keys",
        ));
    }
    let mut conn = state.db.acquire().await?;
    users::audit(
        &mut conn,
        session.user_id,
        Some(session.device_id),
        "keys_created",
    )
    .await?;
    Ok(StatusCode::CREATED)
}

/// `GET /v1/account/keys`
pub async fn get_keys(
    State(state): State<AppState>,
    session: Session,
) -> Result<Json<AccountKeys>, AppError> {
    keys::get(&state.db, session.user_id)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::client(ProblemCode::NotFound, "this account has no keys yet"))
}

/// `PUT /v1/account/keys/master`: needs a fresh Google sign-in.
pub async fn change_master(
    State(state): State<AppState>,
    session: Session,
    Json(body): Json<ChangeMaster>,
) -> Result<StatusCode, AppError> {
    session.require_fresh()?;
    check_kdf(&body.kdf, body.kdf_salt.len())?;
    if body.user_key_by_master.len() != WRAPPED_KEY_LEN {
        return Err(invalid("a wrapped key has the wrong length"));
    }
    if !keys::change_master(&state.db, session.user_id, &body).await? {
        return Err(AppError::client(
            ProblemCode::NotFound,
            "this account has no keys yet",
        ));
    }
    let mut conn = state.db.acquire().await?;
    users::audit(
        &mut conn,
        session.user_id,
        Some(session.device_id),
        "master_changed",
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /v1/account`: removes the account and everything under it.
/// Needs a fresh Google sign-in.
pub async fn delete_account(
    State(state): State<AppState>,
    session: Session,
) -> Result<StatusCode, AppError> {
    session.require_fresh()?;
    users::delete(&state.db, session.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
