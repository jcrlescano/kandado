use serde::{Deserialize, Serialize};

/// The body of every error response, as `application/problem+json` (RFC 9457).
///
/// Clients branch on [`Problem::code`], never on the title or detail, which
/// are for logs and developers. User-facing wording belongs to each client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    pub code: ProblemCode,
    /// A short, fixed summary of the code.
    pub title: String,
    /// The HTTP status, repeated so the body stands alone.
    pub status: u16,
    /// What went wrong this time. Never contains secrets or internal details.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Why a request failed. New codes are added over time; an older client
/// reads any code it does not know as [`ProblemCode::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProblemCode {
    /// The request is malformed or fails validation.
    BadRequest,
    /// No valid access token.
    Unauthorized,
    /// Signed in, but not allowed to do this.
    Forbidden,
    /// A sensitive action needs a Google sign-in from the last few minutes.
    StepUpRequired,
    NotFound,
    /// The change was made against an older revision; pull and retry.
    Conflict,
    PayloadTooLarge,
    RateLimited,
    /// The server failed. Nothing about the cause is exposed.
    Internal,
    /// A dependency such as the database is not reachable.
    Unavailable,
    #[serde(other)]
    Unknown,
}

impl ProblemCode {
    pub fn status(self) -> u16 {
        match self {
            Self::BadRequest => 400,
            Self::Unauthorized => 401,
            Self::Forbidden | Self::StepUpRequired => 403,
            Self::NotFound => 404,
            Self::Conflict => 409,
            Self::PayloadTooLarge => 413,
            Self::RateLimited => 429,
            Self::Internal | Self::Unknown => 500,
            Self::Unavailable => 503,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::BadRequest => "Bad request",
            Self::Unauthorized => "Not signed in",
            Self::Forbidden => "Not allowed",
            Self::StepUpRequired => "Confirmation needed",
            Self::NotFound => "Not found",
            Self::Conflict => "Conflict",
            Self::PayloadTooLarge => "Request too large",
            Self::RateLimited => "Too many requests",
            Self::Internal | Self::Unknown => "Server error",
            Self::Unavailable => "Service unavailable",
        }
    }
}

impl Problem {
    pub fn new(code: ProblemCode) -> Self {
        Self {
            code,
            title: code.title().to_owned(),
            status: code.status(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format() {
        let problem = Problem::new(ProblemCode::Conflict).with_detail("item is at revision 9");
        let json = r#"{"code":"conflict","title":"Conflict","status":409,"detail":"item is at revision 9"}"#;
        assert_eq!(serde_json::to_string(&problem).unwrap(), json);
        assert_eq!(serde_json::from_str::<Problem>(json).unwrap(), problem);
        assert_eq!(
            serde_json::to_string(&Problem::new(ProblemCode::NotFound)).unwrap(),
            r#"{"code":"not_found","title":"Not found","status":404}"#
        );
    }

    #[test]
    fn a_code_from_a_newer_server_reads_as_unknown() {
        let problem: Problem =
            serde_json::from_str(r#"{"code":"plan_limit","title":"Limit","status":402}"#).unwrap();
        assert_eq!(problem.code, ProblemCode::Unknown);
        assert_eq!(problem.status, 402);
    }
}
