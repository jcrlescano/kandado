//! Monitor (D-35 to D-39, E-15): the public lists a device needs to check its
//! passwords, relayed and cached by the server.
//!
//! - `GET /v1/monitor/pwned/{prefix}`: Have I Been Pwned's Pwned Passwords
//!   answer for the first five hex characters of a SHA-1, as plain text
//!   (`SUFFIX:COUNT` lines, padded with zero-count lines).
//! - `GET /v1/monitor/breaches`: Have I Been Pwned's breached sites
//!   ([`BreachList`]). Data CC BY 4.0, Have I Been Pwned.
//! - `GET /v1/monitor/2fa-sites`: sites that offer two-factor codes
//!   ([`TwoFactorSites`]). Data MIT, 2FA Directory by 2factorauth.
//!
//! Devices download the lists whole and match on the device, so the server
//! never learns which sites a person uses.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// What the apps must show beside breach data (CC BY 4.0).
pub const HIBP_ATTRIBUTION: &str = "Breach data from Have I Been Pwned";
/// What the apps must show beside two-factor data (MIT, attribution asked).
pub const TWO_FACTOR_ATTRIBUTION: &str = "Data sourced from 2FA Directory by 2factorauth";

/// One breached site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreachSite {
    /// Have I Been Pwned's stable name, such as `Adobe`.
    pub name: String,
    /// The name to show, such as `Adobe`.
    pub title: String,
    /// The site's domain, such as `adobe.com`.
    pub domain: String,
    /// When the breach happened, `YYYY-MM-DD`.
    pub breach_date: String,
    /// When Have I Been Pwned added it, RFC 3339.
    pub added_date: String,
    /// What was exposed: `Email addresses`, `Passwords`, ...
    pub data_classes: Vec<String>,
}

/// `GET /v1/monitor/breaches`. Verified breaches with a domain only; spam
/// lists, fabricated and malware entries are left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreachList {
    pub breaches: Vec<BreachSite>,
    #[serde(with = "time::serde::rfc3339")]
    pub fetched_at: OffsetDateTime,
}

/// One site that offers two-factor codes (TOTP).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwoFactorSite {
    pub domain: String,
    /// Where the site explains how to turn it on, when the directory knows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
}

/// `GET /v1/monitor/2fa-sites`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwoFactorSites {
    pub sites: Vec<TwoFactorSite>,
    #[serde(with = "time::serde::rfc3339")]
    pub fetched_at: OffsetDateTime,
}
