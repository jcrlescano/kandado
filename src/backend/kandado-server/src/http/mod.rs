//! The HTTP surface: one module per API area.

mod account;
mod auth;
mod devices;
#[cfg(test)]
mod flow_tests;
mod health;
mod sync;
#[cfg(test)]
mod sync_tests;

use std::sync::Arc;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    response::Response,
    routing::{delete, get, post, put},
};
use kandado_protocol::{Problem, ProblemCode};
use sqlx::PgPool;
use tower_http::trace::TraceLayer;

use crate::{
    auth::{google::GoogleProvider, token::TokenSigner},
    config::Config,
    error::problem_response,
};

/// Request bodies are small JSON; anything larger is refused before it is read.
const BODY_LIMIT: usize = 64 * 1024;

/// A batch of changes carries encrypted items, so POST /v1/sync may be larger.
const SYNC_BODY_LIMIT: usize = 16 * 1024 * 1024;

/// What every handler can reach.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub signer: TokenSigner,
    pub google: Arc<GoogleProvider>,
    pub hub: crate::events::Hub,
}

impl AppState {
    pub fn new(db: PgPool, config: Config) -> Self {
        Self {
            signer: TokenSigner::new(&config.token_secret),
            google: Arc::new(GoogleProvider::new(
                config.google.clone(),
                &config.public_url,
            )),
            hub: crate::events::Hub::new(),
            config: Arc::new(config),
            db,
        }
    }
}

pub fn router(state: AppState) -> Router {
    let v1 = Router::new()
        .route("/auth/google/start", get(auth::start))
        .route("/auth/google/callback", get(auth::callback))
        .route("/auth/token", post(auth::exchange))
        .route("/auth/refresh", post(auth::refresh))
        .route("/auth/logout", post(auth::logout))
        .route("/me", get(account::me))
        .route(
            "/account/keys",
            post(account::create_keys).get(account::get_keys),
        )
        .route("/account/keys/master", put(account::change_master))
        .route("/account", delete(account::delete_account))
        .route("/devices", get(devices::list))
        .route("/devices/revoke-others", post(devices::revoke_others))
        .route("/devices/{id}", delete(devices::revoke))
        .route("/sync", get(sync::pull))
        .route(
            "/sync",
            post(sync::push).layer(DefaultBodyLimit::max(SYNC_BODY_LIMIT)),
        )
        .route("/items/{id}/revisions", get(sync::revisions))
        .route("/items/{id}/revisions/{version}", get(sync::revision))
        .route("/events", get(crate::events::events));
    #[cfg(debug_assertions)]
    let v1 = v1.route("/dev/sign-in", get(auth::dev_sign_in));

    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .nest("/v1", v1)
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        // Logs method, path, status and latency. Never headers or bodies.
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn not_found() -> Response {
    problem_response(Problem::new(ProblemCode::NotFound))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::time::Duration;

    use axum::{
        body::Body,
        http::{Request, StatusCode, header},
    };
    use http_body_util::BodyExt;
    use kandado_protocol::{Health, HealthStatus, PROBLEM_CONTENT_TYPE};
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;

    /// A router whose database does not exist: nothing is listening on port 9.
    fn router_without_database() -> Router {
        let db = PgPoolOptions::new()
            .acquire_timeout(Duration::from_millis(300))
            .connect_lazy("postgres://kandado:kandado@127.0.0.1:9/kandado")
            .unwrap();
        router(AppState::new(db, Config::for_tests()))
    }

    async fn get(path: &str) -> (StatusCode, Option<String>, Vec<u8>) {
        let request = Request::get(path).body(Body::empty()).unwrap();
        let response = router_without_database().oneshot(request).await.unwrap();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|value| value.to_str().unwrap().to_owned());
        let status = response.status();
        let body = response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        (status, content_type, body)
    }

    #[tokio::test]
    async fn healthz_answers_without_a_database() {
        let (status, content_type, body) = get("/healthz").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(content_type.as_deref(), Some("application/json"));
        let health: Health = serde_json::from_slice(&body).unwrap();
        assert_eq!(health.status, HealthStatus::Ok);
        assert_eq!(health.version, env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn readyz_is_unavailable_without_a_database() {
        let (status, content_type, body) = get("/readyz").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(content_type.as_deref(), Some(PROBLEM_CONTENT_TYPE));
        let problem: Problem = serde_json::from_slice(&body).unwrap();
        assert_eq!(problem, Problem::new(ProblemCode::Unavailable));
    }

    #[tokio::test]
    async fn unknown_routes_answer_with_a_problem() {
        let (status, content_type, body) = get("/v1/nothing-here").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(content_type.as_deref(), Some(PROBLEM_CONTENT_TYPE));
        let problem: Problem = serde_json::from_slice(&body).unwrap();
        assert_eq!(problem.code, ProblemCode::NotFound);
    }
}
