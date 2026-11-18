//! The Kandado API as types: what clients send and what the server answers.
//!
//! The server, the desktop app and the extension all use these, so a change
//! here is a change to the wire format. Routes are added per phase; this is
//! the base every route shares.
//!
//! The server never looks inside encrypted fields. They travel as [`Opaque`]:
//! base64 in JSON, bytes in memory, and no way to interpret them from here.
#![forbid(unsafe_code)]

mod account;
mod auth;
mod opaque;
mod problem;
mod sync;

pub use account::*;
pub use auth::*;
pub use kandado_model::ItemKind;
pub use opaque::Opaque;
pub use problem::{Problem, ProblemCode};
use serde::{Deserialize, Serialize};
pub use sync::*;

/// Every route is under this prefix, except the health checks.
pub const API_PREFIX: &str = "/v1";

/// Media type of error responses (RFC 9457).
pub const PROBLEM_CONTENT_TYPE: &str = "application/problem+json";

/// `GET /healthz`: the process is up. Says nothing about the database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Health {
    pub status: HealthStatus,
    /// The server's version, such as `0.1.0`.
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Ok,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_wire_format() {
        let health = Health {
            status: HealthStatus::Ok,
            version: "0.1.0".into(),
        };
        let json = r#"{"status":"ok","version":"0.1.0"}"#;
        assert_eq!(serde_json::to_string(&health).unwrap(), json);
        assert_eq!(serde_json::from_str::<Health>(json).unwrap(), health);
    }

    #[test]
    fn item_kinds_use_their_api_names() {
        assert_eq!(
            serde_json::to_string(&ItemKind::Login).unwrap(),
            r#""login""#
        );
    }
}
