//! Signing in: the Google flow's handoff, tokens, and devices.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// Which kind of client is signing in. Decides which redirect addresses the
/// server accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Desktop,
    Extension,
}

/// What the flow is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowPurpose {
    /// A new session on a new or known device.
    SignIn,
    /// A fresh Google sign-in to confirm a sensitive action. The result is a
    /// new access token for the existing session.
    StepUp,
}

/// `GET /v1/auth/google/start` query. The client keeps `verifier` secret and
/// sends only its SHA-256 `challenge` (base64url, no padding).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartQuery {
    pub client: ClientKind,
    pub challenge: String,
    /// Where the server sends the browser afterwards, with `code`.
    pub redirect: String,
    #[serde(default = "default_purpose")]
    pub purpose: FlowPurpose,
    /// For step-up only: the session being confirmed. The server checks that
    /// the Google account that signs in is the session's own.
    #[serde(default)]
    pub access_token: Option<String>,
}

fn default_purpose() -> FlowPurpose {
    FlowPurpose::SignIn
}

/// How a device describes itself when it signs in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// For example a computer name.
    pub name: String,
    /// For example "Windows 11" or "Chrome".
    pub platform: String,
}

/// `POST /v1/auth/token`: trades the single-use code for tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenRequest {
    pub code: String,
    pub verifier: String,
    /// Required for a sign-in, ignored for a step-up.
    #[serde(default)]
    pub device: Option<DeviceInfo>,
}

/// Tokens for a session. A step-up returns no refresh token: the existing
/// session keeps its own.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    /// Seconds until the access token expires.
    pub expires_in: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    pub user_id: Uuid,
    pub device_id: Uuid,
}

impl core::fmt::Debug for TokenResponse {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TokenResponse")
            .field("user_id", &self.user_id)
            .field("device_id", &self.device_id)
            .field("tokens", &"..")
            .finish()
    }
}

/// `POST /v1/auth/refresh`. The refresh token rotates: the response carries a
/// new one and the old one stops working.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

impl core::fmt::Debug for RefreshRequest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("RefreshRequest(..)")
    }
}

/// One signed-in device (D-49).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: Uuid,
    pub name: String,
    pub platform: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub last_seen_at: OffsetDateTime,
    /// The device making this request.
    pub current: bool,
}

/// `GET /v1/devices`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceList {
    pub devices: Vec<Device>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_query_defaults_to_sign_in() {
        let query: StartQuery = serde_json::from_str(
            r#"{"client":"desktop","challenge":"abc","redirect":"http://127.0.0.1:5/callback"}"#,
        )
        .unwrap();
        assert_eq!(query.purpose, FlowPurpose::SignIn);
        assert_eq!(query.access_token, None);
    }

    #[test]
    fn tokens_never_reach_logs() {
        let response = TokenResponse {
            access_token: "secret-access".into(),
            expires_in: 900,
            refresh_token: Some("secret-refresh".into()),
            user_id: Uuid::nil(),
            device_id: Uuid::nil(),
        };
        assert!(!format!("{response:?}").contains("secret"));
        let request = RefreshRequest {
            refresh_token: "secret-refresh".into(),
        };
        assert!(!format!("{request:?}").contains("secret"));
    }
}
