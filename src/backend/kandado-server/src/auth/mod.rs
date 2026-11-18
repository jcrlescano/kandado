//! Who is calling: access tokens, the session extractor, step-up checks and
//! the redirect rules of the sign-in handoff.

pub mod google;
pub mod token;

use axum::{extract::FromRequestParts, http::request::Parts};
use kandado_protocol::{ClientKind, ProblemCode};
use time::{Duration, OffsetDateTime};
use url::Url;
use uuid::Uuid;

use crate::{error::AppError, http::AppState};

/// How recent the Google sign-in must be for a sensitive action.
pub const STEP_UP_WINDOW: Duration = Duration::minutes(5);

/// The caller of an authenticated route.
#[derive(Debug, Clone, Copy)]
pub struct Session {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub auth_time: OffsetDateTime,
}

impl Session {
    /// Fails with `step_up_required` unless the person signed in with Google
    /// within [`STEP_UP_WINDOW`].
    pub fn require_fresh(&self) -> Result<(), AppError> {
        if OffsetDateTime::now_utc() - self.auth_time <= STEP_UP_WINDOW {
            Ok(())
        } else {
            Err(AppError::client(
                ProblemCode::StepUpRequired,
                "sign in with Google again to confirm this",
            ))
        }
    }
}

pub fn bearer(parts: &Parts) -> Option<&str> {
    let value = parts.headers.get(axum::http::header::AUTHORIZATION)?;
    value.to_str().ok()?.strip_prefix("Bearer ")
}

impl FromRequestParts<AppState> for Session {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let unauthorized = || AppError::client(ProblemCode::Unauthorized, "sign in required");
        let token = bearer(parts).ok_or_else(unauthorized)?;
        let claims = state
            .signer
            .verify(token, OffsetDateTime::now_utc())
            .ok_or_else(unauthorized)?;
        // A revoked device stops working at once, not when the token expires.
        if !crate::db::devices::touch_active(&state.db, claims.user_id, claims.device_id).await? {
            return Err(unauthorized());
        }
        Ok(Self {
            user_id: claims.user_id,
            device_id: claims.device_id,
            auth_time: claims.auth_time,
        })
    }
}

/// Whether the browser may be sent to `redirect` at the end of a sign-in.
///
/// Anything looser would let a crafted link deliver the sign-in code to a
/// site the attacker controls. The desktop app listens on a loopback port;
/// an extension uses the address its browser assigns it.
pub fn redirect_allowed(client: ClientKind, redirect: &str, extension_hosts: &[String]) -> bool {
    let Ok(url) = Url::parse(redirect) else {
        return false;
    };
    if !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
    {
        return false;
    }
    match client {
        ClientKind::Desktop => {
            url.scheme() == "http"
                && url.host_str() == Some("127.0.0.1")
                && url.port().is_some()
                && url.path() == "/callback"
        }
        ClientKind::Extension => {
            url.scheme() == "https"
                && url
                    .host_str()
                    .is_some_and(|host| extension_hosts.iter().any(|allowed| allowed == host))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_redirects_must_be_the_loopback_callback() {
        let ok = |url| redirect_allowed(ClientKind::Desktop, url, &[]);
        assert!(ok("http://127.0.0.1:53211/callback"));
        for bad in [
            "http://127.0.0.1/callback",
            "http://127.0.0.1:53211/other",
            "http://127.0.0.1:53211/callback?x=1",
            "http://127.0.0.1:53211/callback#x",
            "https://127.0.0.1:53211/callback",
            "http://localhost:53211/callback",
            "http://127.0.0.1.evil.test:53211/callback",
            "http://user@127.0.0.1:53211/callback",
            "http://evil.test:53211/callback",
            "file:///callback",
            "not a url",
        ] {
            assert!(!ok(bad), "{bad}");
        }
    }

    #[test]
    fn extension_redirects_must_use_a_configured_host() {
        let hosts = vec!["abc.chromiumapp.org".to_owned()];
        let ok = |url| redirect_allowed(ClientKind::Extension, url, &hosts);
        assert!(ok("https://abc.chromiumapp.org/cb"));
        assert!(!ok("https://other.chromiumapp.org/cb"));
        assert!(!ok("http://abc.chromiumapp.org/cb"));
        assert!(!ok("https://abc.chromiumapp.org.evil.test/cb"));
        assert!(!redirect_allowed(
            ClientKind::Extension,
            "https://abc.chromiumapp.org/cb",
            &[]
        ));
    }
}
