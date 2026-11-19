//! The Monitor relay: fetches Have I Been Pwned's Pwned Passwords and breach
//! list and the 2FA Directory's TOTP list, and keeps them for a while.
//!
//! Devices ask this server, never the sources, so the sources see the
//! server's address and not a person's. The lists are handed out whole and
//! matched on the device; a Pwned Passwords question carries only five hex
//! characters of a hash and comes back padded, so no answer says which
//! password was asked about.
//!
//! When a source cannot be reached, a list fetched earlier is served until
//! it is a week old; after that the route answers 503.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use kandado_protocol::{BreachList, BreachSite, TwoFactorSite, TwoFactorSites};
use openidconnect::reqwest;
use serde::Deserialize;
use time::OffsetDateTime;
use tokio::sync::Mutex;

/// How long a Pwned Passwords answer is kept. New leaks are added rarely.
const PWNED_FRESH: Duration = Duration::from_secs(24 * 60 * 60);
/// How many Pwned Passwords answers are kept (about 30 KB each, padded).
const PWNED_KEEP: usize = 4096;
/// How long the breach and two-factor lists are kept before asking again.
const LIST_FRESH: Duration = Duration::from_secs(12 * 60 * 60);
/// How long a list is still served when its source cannot be reached.
const LIST_STALE_LIMIT: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// The most a source may answer, so a broken one cannot exhaust memory.
const MAX_ANSWER: usize = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum MonitorError {
    #[error("the source could not be reached: {0}")]
    Unreachable(String),
    #[error("the source answered with something unexpected: {0}")]
    Unexpected(String),
}

/// Where the sources are. Tests point these at a local fake.
#[derive(Debug, Clone)]
pub struct Sources {
    /// `https://api.pwnedpasswords.com`, without a trailing slash.
    pub pwned: String,
    /// `https://haveibeenpwned.com/api/v3`, without a trailing slash.
    pub breaches: String,
    /// `https://api.2fa.directory/v3/totp.json`.
    pub two_factor: String,
}

impl Default for Sources {
    fn default() -> Self {
        Self {
            pwned: "https://api.pwnedpasswords.com".to_owned(),
            breaches: "https://haveibeenpwned.com/api/v3".to_owned(),
            two_factor: "https://api.2fa.directory/v3/totp.json".to_owned(),
        }
    }
}

struct Kept<T> {
    at: Instant,
    value: Arc<T>,
}

#[derive(Clone)]
pub struct Monitor {
    sources: Arc<Sources>,
    http: reqwest::Client,
    pwned: Arc<Mutex<HashMap<String, Kept<String>>>>,
    breaches: Arc<Mutex<Option<Kept<BreachList>>>>,
    two_factor: Arc<Mutex<Option<Kept<TwoFactorSites>>>>,
}

/// One entry of HIBP's breach list, the fields Kandado uses.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct HibpBreach {
    name: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    domain: String,
    #[serde(default)]
    breach_date: String,
    #[serde(default)]
    added_date: String,
    #[serde(default)]
    data_classes: Vec<String>,
    #[serde(default)]
    is_verified: bool,
    #[serde(default)]
    is_fabricated: bool,
    #[serde(default)]
    is_spam_list: bool,
    #[serde(default)]
    is_malware: bool,
}

/// One entry of the 2FA Directory's v3 list: `["Name", { "domain": …, … }]`.
#[derive(Deserialize)]
struct DirectoryEntry(#[allow(dead_code)] String, DirectorySite);

#[derive(Deserialize)]
struct DirectorySite {
    domain: String,
    #[serde(default, rename = "additional-domains")]
    additional_domains: Vec<String>,
    #[serde(default)]
    documentation: Option<String>,
}

/// True for exactly five hex characters.
pub fn valid_prefix(prefix: &str) -> bool {
    prefix.len() == 5 && prefix.bytes().all(|byte| byte.is_ascii_hexdigit())
}

impl Monitor {
    pub fn new(sources: Sources) -> Self {
        let http = reqwest::ClientBuilder::new()
            .user_agent(concat!("kandado-server/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("an HTTP client with these settings always builds");
        Self {
            sources: Arc::new(sources),
            http,
            pwned: Arc::default(),
            breaches: Arc::default(),
            two_factor: Arc::default(),
        }
    }

    async fn fetch(&self, url: &str, padded: bool) -> Result<String, MonitorError> {
        let mut request = self.http.get(url);
        if padded {
            request = request.header("Add-Padding", "true");
        }
        let response = request
            .send()
            .await
            .map_err(|error| MonitorError::Unreachable(error.to_string()))?;
        if !response.status().is_success() {
            return Err(MonitorError::Unreachable(format!(
                "status {}",
                response.status()
            )));
        }
        if response
            .content_length()
            .is_some_and(|length| length as usize > MAX_ANSWER)
        {
            return Err(MonitorError::Unexpected("the answer is too large".into()));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| MonitorError::Unreachable(error.to_string()))?;
        if bytes.len() > MAX_ANSWER {
            return Err(MonitorError::Unexpected("the answer is too large".into()));
        }
        String::from_utf8(bytes.to_vec())
            .map_err(|_| MonitorError::Unexpected("the answer is not text".into()))
    }

    /// The padded Pwned Passwords answer for a hash prefix. The prefix must
    /// pass [`valid_prefix`].
    pub async fn pwned(&self, prefix: &str) -> Result<Arc<String>, MonitorError> {
        let prefix = prefix.to_ascii_uppercase();
        if let Some(kept) = self.pwned.lock().await.get(&prefix)
            && kept.at.elapsed() < PWNED_FRESH
        {
            return Ok(kept.value.clone());
        }
        let answer = self
            .fetch(&format!("{}/range/{prefix}", self.sources.pwned), true)
            .await?;
        // Every line is SUFFIX:COUNT; anything else is not a Pwned Passwords answer.
        let well_formed = answer.lines().all(|line| {
            let line = line.trim();
            line.is_empty()
                || line.split_once(':').is_some_and(|(suffix, count)| {
                    suffix.len() == 35
                        && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
                        && count.bytes().all(|byte| byte.is_ascii_digit())
                })
        });
        if !well_formed {
            return Err(MonitorError::Unexpected("not a range answer".into()));
        }
        let value = Arc::new(answer);
        let mut kept = self.pwned.lock().await;
        if kept.len() >= PWNED_KEEP {
            kept.retain(|_, entry| entry.at.elapsed() < PWNED_FRESH);
            if kept.len() >= PWNED_KEEP {
                kept.clear();
            }
        }
        kept.insert(
            prefix,
            Kept {
                at: Instant::now(),
                value: value.clone(),
            },
        );
        Ok(value)
    }

    /// Have I Been Pwned's breached sites.
    pub async fn breaches(&self) -> Result<Arc<BreachList>, MonitorError> {
        let mut kept = self.breaches.lock().await;
        if let Some(entry) = kept.as_ref()
            && entry.at.elapsed() < LIST_FRESH
        {
            return Ok(entry.value.clone());
        }
        let fetched = async {
            let text = self
                .fetch(&format!("{}/breaches", self.sources.breaches), false)
                .await?;
            let all: Vec<HibpBreach> = serde_json::from_str(&text)
                .map_err(|error| MonitorError::Unexpected(error.to_string()))?;
            let breaches = all
                .into_iter()
                .filter(|b| {
                    b.is_verified
                        && !b.is_fabricated
                        && !b.is_spam_list
                        && !b.is_malware
                        && !b.domain.trim().is_empty()
                        && b.breach_date.len() >= 10
                })
                .map(|b| BreachSite {
                    title: if b.title.is_empty() {
                        b.name.clone()
                    } else {
                        b.title
                    },
                    name: b.name,
                    domain: b.domain.trim().to_ascii_lowercase(),
                    breach_date: b.breach_date[..10].to_owned(),
                    added_date: b.added_date,
                    data_classes: b.data_classes,
                })
                .collect();
            Ok::<_, MonitorError>(BreachList {
                breaches,
                fetched_at: OffsetDateTime::now_utc(),
            })
        }
        .await;
        keep_or_stale(&mut kept, fetched)
    }

    /// The 2FA Directory's sites that offer TOTP codes.
    pub async fn two_factor(&self) -> Result<Arc<TwoFactorSites>, MonitorError> {
        let mut kept = self.two_factor.lock().await;
        if let Some(entry) = kept.as_ref()
            && entry.at.elapsed() < LIST_FRESH
        {
            return Ok(entry.value.clone());
        }
        let fetched = async {
            let text = self.fetch(&self.sources.two_factor, false).await?;
            let entries: Vec<DirectoryEntry> = serde_json::from_str(&text)
                .map_err(|error| MonitorError::Unexpected(error.to_string()))?;
            let mut sites = Vec::new();
            for DirectoryEntry(_, site) in entries {
                let documentation = site.documentation.filter(|url| url.starts_with("https://"));
                for domain in std::iter::once(site.domain).chain(site.additional_domains) {
                    let domain = domain.trim().to_ascii_lowercase();
                    if !domain.is_empty() {
                        sites.push(TwoFactorSite {
                            domain,
                            documentation: documentation.clone(),
                        });
                    }
                }
            }
            Ok::<_, MonitorError>(TwoFactorSites {
                sites,
                fetched_at: OffsetDateTime::now_utc(),
            })
        }
        .await;
        keep_or_stale(&mut kept, fetched)
    }
}

/// Keeps a fresh list, or falls back to the one kept earlier while it is
/// less than a week old.
fn keep_or_stale<T>(
    kept: &mut Option<Kept<T>>,
    fetched: Result<T, MonitorError>,
) -> Result<Arc<T>, MonitorError> {
    match fetched {
        Ok(value) => {
            let value = Arc::new(value);
            *kept = Some(Kept {
                at: Instant::now(),
                value: value.clone(),
            });
            Ok(value)
        }
        Err(error) => match kept.as_ref() {
            Some(entry) if entry.at.elapsed() < LIST_STALE_LIMIT => {
                tracing::warn!(%error, "serving an older Monitor list");
                Ok(entry.value.clone())
            }
            _ => Err(error),
        },
    }
}
