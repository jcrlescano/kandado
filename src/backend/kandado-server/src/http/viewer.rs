//! The secure link viewer page, `GET /l/{id}`, and its files under
//! `/l/assets/`. A static page that decrypts in the browser with the WASM
//! core (built by `scripts/build-viewer.mjs`). The server only hands out the
//! files; the link key in the fragment never reaches it.
//!
//! The files are read once at startup from `KANDADO_VIEWER_DIR`. If they are
//! missing the API still runs and `/l/{id}` answers 503.

use std::{collections::HashMap, path::Path, sync::Arc};

use axum::{
    body::Bytes,
    extract::{Path as UrlPath, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use kandado_protocol::{Problem, ProblemCode};

use super::AppState;
use crate::error::problem_response;

/// The page's files, by name, with their media type.
const FILES: &[(&str, &str)] = &[
    ("index.html", "text/html; charset=utf-8"),
    ("viewer.js", "text/javascript; charset=utf-8"),
    ("viewer.css", "text/css; charset=utf-8"),
    ("tokens.css", "text/css; charset=utf-8"),
    ("hardware.css", "text/css; charset=utf-8"),
    ("logo-light.svg", "image/svg+xml"),
    ("logo-dark.svg", "image/svg+xml"),
    ("SchibstedGrotesk.ttf", "font/ttf"),
    ("MartianMono.ttf", "font/ttf"),
    ("kandado_wasm.js", "text/javascript; charset=utf-8"),
    ("kandado_wasm_bg.wasm", "application/wasm"),
];

/// Scripts and styles from this server only, WebAssembly allowed, requests to
/// this server only, no framing, no forms, no plugins.
const CSP: &str = "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; \
    style-src 'self'; font-src 'self'; img-src 'self'; connect-src 'self'; \
    base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

#[derive(Clone, Default)]
pub struct Viewer {
    files: Arc<HashMap<&'static str, (&'static str, Bytes)>>,
}

impl Viewer {
    /// Reads the files in `dir`. Missing files are logged; the page works
    /// only when every one is there.
    pub fn load(dir: Option<&Path>) -> Self {
        let Some(dir) = dir else {
            return Self::default();
        };
        let mut files = HashMap::new();
        let mut missing = Vec::new();
        for (name, media) in FILES {
            match std::fs::read(dir.join(name)) {
                Ok(bytes) => {
                    files.insert(*name, (*media, Bytes::from(bytes)));
                }
                Err(_) => missing.push(*name),
            }
        }
        if !missing.is_empty() {
            tracing::warn!(
                dir = %dir.display(),
                missing = ?missing,
                "the secure link viewer is not built; run node scripts/build-viewer.mjs"
            );
            return Self::default();
        }
        Self {
            files: Arc::new(files),
        }
    }

    fn file(&self, name: &str) -> Option<&(&'static str, Bytes)> {
        self.files.get(name)
    }
}

fn serve(viewer: &Viewer, name: &str, cache: &'static str) -> Response {
    let Some((media, bytes)) = viewer.file(name) else {
        return problem_response(
            Problem::new(ProblemCode::Unavailable).with_detail("the link viewer is not available"),
        );
    };
    let mut response = (StatusCode::OK, bytes.clone()).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(media));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        "x-robots-tag",
        HeaderValue::from_static("noindex, nofollow"),
    );
    response
}

/// `GET /l/{id}`: the page. The id is read by the page itself.
pub async fn page(State(state): State<AppState>, UrlPath(_id): UrlPath<String>) -> Response {
    serve(&state.viewer, "index.html", "no-store")
}

/// `GET /l/assets/{name}`
pub async fn asset(State(state): State<AppState>, UrlPath(name): UrlPath<String>) -> Response {
    if name == "index.html" || state.viewer.file(&name).is_none() {
        return problem_response(Problem::new(ProblemCode::NotFound));
    }
    serve(&state.viewer, &name, "public, max-age=3600")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::{
        body::Body,
        http::{Request, StatusCode, header},
    };
    use http_body_util::BodyExt;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use super::*;
    use crate::{config::Config, http::router};

    /// A folder with every viewer file, each holding its own name.
    fn built() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kandado-viewer-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, _) in FILES {
            std::fs::write(dir.join(name), name.as_bytes()).unwrap();
        }
        dir
    }

    async fn get(
        dir: Option<std::path::PathBuf>,
        path: &str,
    ) -> (StatusCode, header::HeaderMap, Vec<u8>) {
        let db = PgPoolOptions::new()
            .acquire_timeout(Duration::from_millis(300))
            .connect_lazy("postgres://kandado:kandado@127.0.0.1:9/kandado")
            .unwrap();
        let mut config = Config::for_tests();
        config.viewer_dir = dir;
        let app = router(AppState::new(db, config));
        let response = app
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        (status, headers, body)
    }

    #[tokio::test]
    async fn the_page_is_served_for_any_link_with_a_strict_policy() {
        let dir = built();
        let (status, headers, body) =
            get(Some(dir.clone()), "/l/0190d7b8-0000-7000-8000-000000000001").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, b"index.html");
        assert_eq!(headers[header::CONTENT_TYPE], "text/html; charset=utf-8");
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        assert_eq!(headers[header::REFERRER_POLICY], "no-referrer");
        assert_eq!(headers[header::X_FRAME_OPTIONS], "DENY");
        let csp = headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
        assert!(csp.contains("default-src 'none'") && csp.contains("frame-ancestors 'none'"));

        let (status, headers, body) =
            get(Some(dir.clone()), "/l/assets/kandado_wasm_bg.wasm").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, b"kandado_wasm_bg.wasm");
        assert_eq!(headers[header::CONTENT_TYPE], "application/wasm");

        // Only the listed files, and never by another path.
        for path in [
            "/l/assets/secret.txt",
            "/l/assets/index.html",
            "/l/assets/..%2Fviewer.js",
        ] {
            assert_eq!(
                get(Some(dir.clone()), path).await.0,
                StatusCode::NOT_FOUND,
                "{path}"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn without_a_built_viewer_the_page_is_unavailable() {
        let (status, _, _) = get(None, "/l/0190d7b8-0000-7000-8000-000000000001").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        let empty = std::env::temp_dir().join(format!("kandado-viewer-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&empty).unwrap();
        let (status, _, _) = get(Some(empty.clone()), "/l/x").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        std::fs::remove_dir_all(empty).unwrap();
    }
}
