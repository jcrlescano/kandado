//! The client for the Kandado backend. It owns the access and refresh tokens:
//! when the server answers 401 it refreshes once and retries, and it reports
//! every new refresh token through a callback so the caller can store it.

use std::{fmt, sync::Arc, time::Duration};

use kandado_protocol::{
    AccountKeys, ChangeMaster, ClientKind, DeviceInfo, DeviceList, FlowPurpose, ItemVersions, Me,
    PROBLEM_CONTENT_TYPE, Problem, ProblemCode, PullResponse, PushRequest, PushResponse,
    RefreshRequest, StartQuery, TokenRequest, TokenResponse,
};
use reqwest::{Method, StatusCode};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::Mutex;
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

/// The pair that proves a session. Never printed.
#[derive(Clone)]
pub struct Tokens {
    pub access: Zeroizing<String>,
    pub refresh: Zeroizing<String>,
}

impl fmt::Debug for Tokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Tokens(..)")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// The server answered with an error the caller can act on.
    #[error("{}: {}", .0.title, .0.detail.as_deref().unwrap_or(""))]
    Problem(Problem),
    /// The server could not be reached.
    #[error("the server could not be reached: {0}")]
    Offline(String),
    /// The server's answer was not what the protocol says.
    #[error("the server sent something unexpected: {0}")]
    Unexpected(String),
    /// There is no session, or it was ended on another device.
    #[error("you are signed out")]
    SignedOut,
}

impl ApiError {
    pub fn code(&self) -> Option<ProblemCode> {
        match self {
            Self::Problem(problem) => Some(problem.code),
            _ => None,
        }
    }

    /// True when retrying later might work.
    pub fn is_offline(&self) -> bool {
        matches!(self, Self::Offline(_))
    }
}

type OnTokens = Arc<dyn Fn(&Tokens) + Send + Sync>;

pub struct Api {
    http: reqwest::Client,
    base: Url,
    tokens: Mutex<Option<Tokens>>,
    on_tokens: OnTokens,
}

impl Api {
    /// `base` is the server's address, such as `https://api.kandado.app`.
    /// `on_tokens` is called whenever the refresh token changes.
    pub fn new(
        base: &str,
        on_tokens: impl Fn(&Tokens) + Send + Sync + 'static,
    ) -> Result<Self, ApiError> {
        let mut base = Url::parse(base).map_err(|error| ApiError::Unexpected(error.to_string()))?;
        if !base.path().ends_with('/') {
            let path = format!("{}/", base.path());
            base.set_path(&path);
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("Kandado-Desktop/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| ApiError::Unexpected(error.to_string()))?;
        Ok(Self {
            http,
            base,
            tokens: Mutex::new(None),
            on_tokens: Arc::new(on_tokens),
        })
    }

    pub async fn set_tokens(&self, tokens: Option<Tokens>) {
        *self.tokens.lock().await = tokens;
    }

    pub async fn has_tokens(&self) -> bool {
        self.tokens.lock().await.is_some()
    }

    fn url(&self, path: &str) -> Url {
        self.base
            .join(path.trim_start_matches('/'))
            .expect("API paths are fixed strings")
    }

    /// The address to open in the browser to start Google sign-in.
    pub fn google_start_url(
        &self,
        challenge: &str,
        redirect: &str,
        purpose: FlowPurpose,
        access_token: Option<&str>,
    ) -> Url {
        let query = StartQuery {
            client: ClientKind::Desktop,
            challenge: challenge.to_owned(),
            redirect: redirect.to_owned(),
            purpose,
            access_token: access_token.map(str::to_owned),
        };
        let mut url = self.url("v1/auth/google/start");
        {
            let mut pairs = url.query_pairs_mut();
            pairs
                .append_pair("client", "desktop")
                .append_pair("challenge", &query.challenge)
                .append_pair("redirect", &query.redirect);
            if purpose == FlowPurpose::StepUp {
                pairs.append_pair("purpose", "step_up");
            }
            if let Some(token) = &query.access_token {
                pairs.append_pair("access_token", token);
            }
        }
        url
    }

    pub async fn access_token(&self) -> Option<String> {
        self.tokens
            .lock()
            .await
            .as_ref()
            .map(|tokens| tokens.access.to_string())
    }

    async fn raw(
        &self,
        method: Method,
        path: &str,
        body: Option<&[u8]>,
        access: Option<&str>,
    ) -> Result<(StatusCode, Vec<u8>), ApiError> {
        let mut request = self.http.request(method, self.url(path));
        if let Some(access) = access {
            request = request.bearer_auth(access);
        }
        if let Some(body) = body {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.to_vec());
        }
        let response = request
            .send()
            .await
            .map_err(|error| ApiError::Offline(error.without_url().to_string()))?;
        let status = response.status();
        let is_problem = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with(PROBLEM_CONTENT_TYPE));
        let bytes = response
            .bytes()
            .await
            .map_err(|error| ApiError::Offline(error.without_url().to_string()))?
            .to_vec();
        if status.is_success() {
            return Ok((status, bytes));
        }
        if is_problem && let Ok(problem) = serde_json::from_slice::<Problem>(&bytes) {
            return Err(ApiError::Problem(problem));
        }
        if status.is_server_error() {
            return Err(ApiError::Offline(format!("the server answered {status}")));
        }
        Err(ApiError::Unexpected(format!(
            "the server answered {status}"
        )))
    }

    /// Sends an authenticated request, refreshing the session once on a 401.
    async fn authed(
        &self,
        method: Method,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, ApiError> {
        let used = self.access_token().await.ok_or(ApiError::SignedOut)?;
        let first = self
            .raw(method.clone(), path, body.as_deref(), Some(&used))
            .await;
        match first {
            Err(ApiError::Problem(problem)) if problem.code == ProblemCode::Unauthorized => {}
            other => return other.map(|(_, bytes)| bytes),
        }
        let renewed = self.refresh_after(&used).await?;
        self.raw(method, path, body.as_deref(), Some(&renewed))
            .await
            .map(|(_, bytes)| bytes)
            .map_err(|error| match error {
                ApiError::Problem(problem) if problem.code == ProblemCode::Unauthorized => {
                    ApiError::SignedOut
                }
                other => other,
            })
    }

    /// Refreshes the session unless another call already did. Returns the
    /// access token to use.
    async fn refresh_after(&self, stale_access: &str) -> Result<String, ApiError> {
        let mut guard = self.tokens.lock().await;
        let current = guard.clone().ok_or(ApiError::SignedOut)?;
        if current.access.as_str() != stale_access {
            return Ok(current.access.to_string());
        }
        let body = serde_json::to_vec(&RefreshRequest {
            refresh_token: current.refresh.to_string(),
        })
        .map_err(|error| ApiError::Unexpected(error.to_string()))?;
        let reply = self
            .raw(Method::POST, "v1/auth/refresh", Some(&body), None)
            .await;
        let (_, bytes) = match reply {
            Ok(reply) => reply,
            Err(ApiError::Problem(problem)) if problem.code == ProblemCode::Unauthorized => {
                // Another device revoked this one, or the token was replayed.
                *guard = None;
                return Err(ApiError::SignedOut);
            }
            Err(error) => return Err(error),
        };
        let response: TokenResponse = decode(&bytes)?;
        let refresh = response
            .refresh_token
            .ok_or_else(|| ApiError::Unexpected("no refresh token".into()))?;
        let tokens = Tokens {
            access: Zeroizing::new(response.access_token),
            refresh: Zeroizing::new(refresh),
        };
        (self.on_tokens)(&tokens);
        let access = tokens.access.to_string();
        *guard = Some(tokens);
        Ok(access)
    }

    async fn get_json<R: DeserializeOwned>(&self, path: &str) -> Result<R, ApiError> {
        decode(&self.authed(Method::GET, path, None).await?)
    }

    async fn send_json<B: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: &B,
    ) -> Result<(), ApiError> {
        let body =
            serde_json::to_vec(body).map_err(|error| ApiError::Unexpected(error.to_string()))?;
        self.authed(method, path, Some(body)).await.map(|_| ())
    }

    // ---- sign-in -------------------------------------------------------

    /// Trades the code from the browser for a session on this device.
    pub async fn sign_in(
        &self,
        code: &str,
        verifier: &str,
        device: DeviceInfo,
    ) -> Result<TokenResponse, ApiError> {
        let body = serde_json::to_vec(&TokenRequest {
            code: code.to_owned(),
            verifier: verifier.to_owned(),
            device: Some(device),
        })
        .map_err(|error| ApiError::Unexpected(error.to_string()))?;
        let (_, bytes) = self
            .raw(Method::POST, "v1/auth/token", Some(&body), None)
            .await?;
        let response: TokenResponse = decode(&bytes)?;
        let refresh = response
            .refresh_token
            .clone()
            .ok_or_else(|| ApiError::Unexpected("no refresh token".into()))?;
        let tokens = Tokens {
            access: Zeroizing::new(response.access_token.clone()),
            refresh: Zeroizing::new(refresh),
        };
        (self.on_tokens)(&tokens);
        *self.tokens.lock().await = Some(tokens);
        Ok(response)
    }

    /// Completes a step-up: keeps the session, with a fresh access token.
    pub async fn confirm(&self, code: &str, verifier: &str) -> Result<(), ApiError> {
        let body = serde_json::to_vec(&TokenRequest {
            code: code.to_owned(),
            verifier: verifier.to_owned(),
            device: None,
        })
        .map_err(|error| ApiError::Unexpected(error.to_string()))?;
        let (_, bytes) = self
            .raw(Method::POST, "v1/auth/token", Some(&body), None)
            .await?;
        let response: TokenResponse = decode(&bytes)?;
        let mut guard = self.tokens.lock().await;
        let current = guard.as_mut().ok_or(ApiError::SignedOut)?;
        current.access = Zeroizing::new(response.access_token);
        Ok(())
    }

    /// Ends this device's session on the server, then forgets the tokens.
    /// Offline, the tokens are forgotten anyway: signing out must not wait for
    /// the network.
    pub async fn logout(&self) {
        let _ = self.authed(Method::POST, "v1/auth/logout", None).await;
        *self.tokens.lock().await = None;
    }

    // ---- account -------------------------------------------------------

    pub async fn me(&self) -> Result<Me, ApiError> {
        self.get_json("v1/me").await
    }

    pub async fn create_keys(&self, keys: &AccountKeys) -> Result<(), ApiError> {
        self.send_json(Method::POST, "v1/account/keys", keys).await
    }

    pub async fn keys(&self) -> Result<AccountKeys, ApiError> {
        self.get_json("v1/account/keys").await
    }

    pub async fn change_master(&self, change: &ChangeMaster) -> Result<(), ApiError> {
        self.send_json(Method::PUT, "v1/account/keys/master", change)
            .await
    }

    pub async fn delete_account(&self) -> Result<(), ApiError> {
        self.authed(Method::DELETE, "v1/account", None)
            .await
            .map(|_| ())
    }

    // ---- sync ----------------------------------------------------------

    /// What changed on the server after `cursor`.
    pub async fn sync_pull(&self, cursor: i64, limit: u32) -> Result<PullResponse, ApiError> {
        self.get_json(&format!("v1/sync?cursor={cursor}&limit={limit}"))
            .await
    }

    /// Sends changes. Each is answered on its own (see [`PushResponse`]).
    pub async fn sync_push(&self, request: &PushRequest) -> Result<PushResponse, ApiError> {
        let body =
            serde_json::to_vec(request).map_err(|error| ApiError::Unexpected(error.to_string()))?;
        decode(&self.authed(Method::POST, "v1/sync", Some(body)).await?)
    }

    /// Every version the server has of an item, newest first.
    pub async fn item_versions(&self, item: Uuid) -> Result<ItemVersions, ApiError> {
        self.get_json(&format!("v1/items/{item}/revisions")).await
    }

    /// Opens the connection that says when something changed on the server.
    pub async fn open_events(&self) -> Result<crate::events::EventStream, ApiError> {
        let used = self.access_token().await.ok_or(ApiError::SignedOut)?;
        let address = self.url("v1/events");
        match crate::events::connect(&address, &used).await {
            Err(crate::events::ConnectError::Unauthorized) => {
                let renewed = self.refresh_after(&used).await?;
                crate::events::connect(&address, &renewed)
                    .await
                    .map_err(crate::events::ConnectError::into_api)
            }
            other => other.map_err(crate::events::ConnectError::into_api),
        }
    }

    // ---- devices -------------------------------------------------------

    pub async fn devices(&self) -> Result<DeviceList, ApiError> {
        self.get_json("v1/devices").await
    }

    pub async fn revoke_device(&self, id: Uuid) -> Result<(), ApiError> {
        self.authed(Method::DELETE, &format!("v1/devices/{id}"), None)
            .await
            .map(|_| ())
    }

    pub async fn revoke_other_devices(&self) -> Result<(), ApiError> {
        self.authed(Method::POST, "v1/devices/revoke-others", None)
            .await
            .map(|_| ())
    }
}

fn decode<R: DeserializeOwned>(bytes: &[u8]) -> Result<R, ApiError> {
    serde_json::from_slice(bytes).map_err(|error| ApiError::Unexpected(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_never_print() {
        let tokens = Tokens {
            access: Zeroizing::new("secret-access".into()),
            refresh: Zeroizing::new("secret-refresh".into()),
        };
        assert!(!format!("{tokens:?}").contains("secret"));
    }

    #[test]
    fn paths_join_under_the_base_address() {
        let api = Api::new("https://api.example.test/prefix", |_| {}).unwrap();
        assert_eq!(
            api.url("/v1/me").as_str(),
            "https://api.example.test/prefix/v1/me"
        );
        let start = api.google_start_url(
            "abc",
            "http://127.0.0.1:5/callback",
            FlowPurpose::SignIn,
            None,
        );
        assert_eq!(start.path(), "/prefix/v1/auth/google/start");
        let pairs: Vec<_> = start.query_pairs().collect();
        assert!(pairs.iter().any(|(k, v)| k == "challenge" && v == "abc"));
        assert!(!pairs.iter().any(|(k, _)| k == "purpose"));
    }

    #[tokio::test]
    async fn requests_without_a_session_say_so() {
        let api = Api::new("http://127.0.0.1:9", |_| {}).unwrap();
        assert!(matches!(api.me().await, Err(ApiError::SignedOut)));
    }

    #[tokio::test]
    async fn an_unreachable_server_is_offline() {
        let api = Api::new("http://127.0.0.1:9", |_| {}).unwrap();
        let error = api
            .sign_in(
                "code",
                "verifier",
                DeviceInfo {
                    name: "PC".into(),
                    platform: "Windows".into(),
                },
            )
            .await
            .unwrap_err();
        assert!(error.is_offline(), "{error}");
    }
}
