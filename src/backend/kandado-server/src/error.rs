//! The one error type handlers return. It always leaves the server as
//! `application/problem+json`, and never carries internal details with it.

use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use kandado_protocol::{PROBLEM_CONTENT_TYPE, Problem, ProblemCode};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// A failure the client can act on. The detail is safe to show.
    #[error("{code:?}: {detail}")]
    Client { code: ProblemCode, detail: String },
    /// Something on the server failed that is not the database. Logged only.
    #[error("internal: {0}")]
    Internal(String),
    /// The database failed or is unreachable. Logged; the client learns nothing more.
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
}

impl AppError {
    pub fn client(code: ProblemCode, detail: impl Into<String>) -> Self {
        Self::Client {
            code,
            detail: detail.into(),
        }
    }

    fn problem(&self) -> Problem {
        match self {
            Self::Client { code, detail } => Problem::new(*code).with_detail(detail.clone()),
            Self::Internal(_) => Problem::new(ProblemCode::Internal),
            Self::Database(sqlx::Error::PoolTimedOut | sqlx::Error::Io(_)) => {
                Problem::new(ProblemCode::Unavailable)
            }
            Self::Database(_) => Problem::new(ProblemCode::Internal),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let problem = self.problem();
        if problem.status >= 500 {
            tracing::error!(error = %self, "request failed");
        }
        problem_response(problem)
    }
}

pub fn problem_response(problem: Problem) -> Response {
    let status = StatusCode::from_u16(problem.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (
        status,
        [(header::CONTENT_TYPE, PROBLEM_CONTENT_TYPE)],
        Json(problem),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;

    use super::*;

    async fn body(response: Response) -> Problem {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn client_errors_keep_their_detail() {
        let response =
            AppError::client(ProblemCode::BadRequest, "cursor must be a number").into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            PROBLEM_CONTENT_TYPE
        );
        let problem = body(response).await;
        assert_eq!(problem.code, ProblemCode::BadRequest);
        assert_eq!(problem.detail.as_deref(), Some("cursor must be a number"));
    }

    #[tokio::test]
    async fn database_errors_reveal_nothing() {
        let response =
            AppError::from(sqlx::Error::Protocol("secret table missing".into())).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body(response).await, Problem::new(ProblemCode::Internal));

        let response = AppError::from(sqlx::Error::PoolTimedOut).into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body(response).await, Problem::new(ProblemCode::Unavailable));
    }
}
