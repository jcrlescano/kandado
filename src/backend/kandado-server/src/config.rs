//! Configuration, read once at startup from environment variables.
//! For local development they come from a `.env` file (see `.env.example`).

use std::{fmt, net::SocketAddr, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0} is not set")]
    Missing(&'static str),
    #[error("{0} is not valid: {1}")]
    Invalid(&'static str, String),
}

#[derive(Clone)]
pub struct Config {
    /// Address to listen on. `KANDADO_BIND`, default `127.0.0.1:8080`.
    pub bind: SocketAddr,
    /// PostgreSQL connection string. `DATABASE_URL`, required.
    pub database_url: String,
    /// Largest number of database connections. `KANDADO_DB_MAX_CONNECTIONS`, default 10.
    pub db_max_connections: u32,
    /// `KANDADO_LOG_FORMAT`: `json` (default) or `text`.
    pub log_format: LogFormat,
    /// Secret that signs access tokens, at least 32 bytes. `KANDADO_TOKEN_SECRET`, required.
    pub token_secret: Vec<u8>,
    /// Address clients reach this server at; Google redirects back to
    /// `{public_url}/v1/auth/google/callback`. `KANDADO_PUBLIC_URL`, default `http://127.0.0.1:8080`.
    pub public_url: String,
    /// Google OAuth client. Both unset means Google sign-in is unavailable.
    /// `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET`.
    pub google: Option<GoogleConfig>,
    /// Browser extension redirect hosts, comma separated, such as
    /// `abcdef.chromiumapp.org`. `KANDADO_EXTENSION_REDIRECT_HOSTS`, default none.
    pub extension_redirect_hosts: Vec<String>,
    /// Turns on the development sign-in route that stands in for Google.
    /// `KANDADO_DEV_AUTH=1`. Only has an effect in debug builds.
    pub dev_auth: bool,
    /// The built secure link viewer (`scripts/build-viewer.mjs`).
    /// `KANDADO_VIEWER_DIR`, default `src/backend/viewer/dist`; empty turns it off.
    pub viewer_dir: Option<PathBuf>,
}

#[derive(Clone)]
pub struct GoogleConfig {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Json,
    Text,
}

impl Config {
    /// A configuration for tests: dev sign-in on, nothing else set.
    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self {
            bind: ([127, 0, 0, 1], 0).into(),
            database_url: String::new(),
            db_max_connections: 5,
            log_format: LogFormat::Text,
            token_secret: b"test-secret-test-secret-test-secret!".to_vec(),
            public_url: "http://127.0.0.1:8080".to_owned(),
            google: None,
            extension_redirect_hosts: vec!["test.chromiumapp.org".to_owned()],
            dev_auth: true,
            viewer_dir: None,
        }
    }

    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup(get: impl Fn(&'static str) -> Option<String>) -> Result<Self, ConfigError> {
        let parsed = |name, default: &str| -> Result<_, ConfigError> {
            let value = get(name).unwrap_or_else(|| default.to_owned());
            Ok((name, value))
        };

        let (name, value) = parsed("KANDADO_BIND", "127.0.0.1:8080")?;
        let bind = value
            .parse()
            .map_err(|_| ConfigError::Invalid(name, value))?;

        let (name, value) = parsed("KANDADO_DB_MAX_CONNECTIONS", "10")?;
        let db_max_connections = match value.parse() {
            Ok(count) if count > 0 => count,
            _ => return Err(ConfigError::Invalid(name, value)),
        };

        let (name, value) = parsed("KANDADO_LOG_FORMAT", "json")?;
        let log_format = match value.as_str() {
            "json" => LogFormat::Json,
            "text" => LogFormat::Text,
            _ => return Err(ConfigError::Invalid(name, value)),
        };

        let database_url = get("DATABASE_URL")
            .filter(|url| !url.is_empty())
            .ok_or(ConfigError::Missing("DATABASE_URL"))?;

        let token_secret = get("KANDADO_TOKEN_SECRET")
            .filter(|secret| !secret.is_empty())
            .ok_or(ConfigError::Missing("KANDADO_TOKEN_SECRET"))?
            .into_bytes();
        if token_secret.len() < 32 {
            return Err(ConfigError::Invalid(
                "KANDADO_TOKEN_SECRET",
                "must be at least 32 bytes".into(),
            ));
        }

        let public_url = get("KANDADO_PUBLIC_URL")
            .unwrap_or_else(|| "http://127.0.0.1:8080".to_owned())
            .trim_end_matches('/')
            .to_owned();
        if url::Url::parse(&public_url).is_err() {
            return Err(ConfigError::Invalid("KANDADO_PUBLIC_URL", public_url));
        }

        let google = match (get("GOOGLE_CLIENT_ID"), get("GOOGLE_CLIENT_SECRET")) {
            (Some(client_id), Some(client_secret))
                if !client_id.is_empty() && !client_secret.is_empty() =>
            {
                Some(GoogleConfig {
                    client_id,
                    client_secret,
                })
            }
            _ => None,
        };

        let extension_redirect_hosts = get("KANDADO_EXTENSION_REDIRECT_HOSTS")
            .unwrap_or_default()
            .split(',')
            .map(|host| host.trim().to_owned())
            .filter(|host| !host.is_empty())
            .collect();

        let dev_auth = matches!(get("KANDADO_DEV_AUTH").as_deref(), Some("1" | "true"));

        let viewer_dir = match get("KANDADO_VIEWER_DIR") {
            Some(dir) if dir.is_empty() => None,
            Some(dir) => Some(PathBuf::from(dir)),
            None => Some(PathBuf::from("src/backend/viewer/dist")),
        };

        Ok(Self {
            bind,
            database_url,
            db_max_connections,
            log_format,
            token_secret,
            public_url,
            google,
            extension_redirect_hosts,
            dev_auth,
            viewer_dir,
        })
    }
}

/// The connection string holds the database password, so it is never printed.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("bind", &self.bind)
            .field("database_url", &"..")
            .field("db_max_connections", &self.db_max_connections)
            .field("log_format", &self.log_format)
            .field("token_secret", &"..")
            .field("google", &self.google.as_ref().map(|_| ".."))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    fn lookup(
        pairs: &'static [(&'static str, &'static str)],
    ) -> impl Fn(&'static str) -> Option<String> {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn only_the_database_url_and_token_secret_are_required() {
        let config = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://u:p@db/kandado"),
            ("KANDADO_TOKEN_SECRET", SECRET),
        ]))
        .unwrap();
        assert_eq!(config.bind.to_string(), "127.0.0.1:8080");
        assert_eq!(config.db_max_connections, 10);
        assert_eq!(config.log_format, LogFormat::Json);
        assert_eq!(config.public_url, "http://127.0.0.1:8080");
        assert!(config.google.is_none());
        assert!(!config.dev_auth);

        let missing = Config::from_lookup(lookup(&[])).unwrap_err();
        assert_eq!(missing.to_string(), "DATABASE_URL is not set");
        let missing =
            Config::from_lookup(lookup(&[("DATABASE_URL", "postgres://db")])).unwrap_err();
        assert_eq!(missing.to_string(), "KANDADO_TOKEN_SECRET is not set");
    }

    #[test]
    fn every_setting_can_be_overridden() {
        let config = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://u:p@db/kandado"),
            ("KANDADO_TOKEN_SECRET", SECRET),
            ("KANDADO_BIND", "0.0.0.0:9000"),
            ("KANDADO_DB_MAX_CONNECTIONS", "3"),
            ("KANDADO_LOG_FORMAT", "text"),
            ("KANDADO_PUBLIC_URL", "https://api.example.test/"),
            ("GOOGLE_CLIENT_ID", "id"),
            ("GOOGLE_CLIENT_SECRET", "secret"),
            (
                "KANDADO_EXTENSION_REDIRECT_HOSTS",
                "a.chromiumapp.org, b.extensions.allizom.org",
            ),
            ("KANDADO_DEV_AUTH", "1"),
        ]))
        .unwrap();
        assert_eq!(config.bind.to_string(), "0.0.0.0:9000");
        assert_eq!(config.db_max_connections, 3);
        assert_eq!(config.log_format, LogFormat::Text);
        assert_eq!(config.public_url, "https://api.example.test");
        assert!(config.google.is_some());
        assert_eq!(
            config.extension_redirect_hosts,
            ["a.chromiumapp.org", "b.extensions.allizom.org"]
        );
        assert!(config.dev_auth);
    }

    #[test]
    fn invalid_values_are_named() {
        for (name, value) in [
            ("KANDADO_BIND", "not an address"),
            ("KANDADO_DB_MAX_CONNECTIONS", "0"),
            ("KANDADO_LOG_FORMAT", "xml"),
            ("KANDADO_TOKEN_SECRET", "too short"),
            ("KANDADO_PUBLIC_URL", "not a url"),
        ] {
            let mut pairs = vec![(name, value)];
            if name != "KANDADO_TOKEN_SECRET" {
                pairs.push(("KANDADO_TOKEN_SECRET", SECRET));
            }
            pairs.push(("DATABASE_URL", "postgres://db"));
            let pairs: &'static [_] = Box::leak(pairs.into_boxed_slice());
            let error = Config::from_lookup(lookup(pairs)).unwrap_err();
            assert!(error.to_string().starts_with(name), "{error}");
        }
    }

    #[test]
    fn debug_output_never_shows_secrets() {
        let config = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://u:hunter2@db/kandado"),
            ("KANDADO_TOKEN_SECRET", SECRET),
            ("GOOGLE_CLIENT_ID", "id"),
            ("GOOGLE_CLIENT_SECRET", "google-secret"),
        ]))
        .unwrap();
        let shown = format!("{config:?}");
        assert!(!shown.contains("hunter2"));
        assert!(!shown.contains(SECRET));
        assert!(!shown.contains("google-secret"));
    }
}
