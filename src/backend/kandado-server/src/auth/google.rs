//! The Google leg of sign-in: build the authorisation URL, and turn the code
//! Google sends back into a verified identity.
//!
//! The client secret stays here on the server. The ID token's signature,
//! issuer, audience, expiry and nonce are all checked by `openidconnect`;
//! `email_verified` is checked here.

use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet, EndpointNotSet,
    EndpointSet, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    TokenResponse,
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
    reqwest,
};
use tokio::sync::OnceCell;
use url::Url;

use crate::config::GoogleConfig;

const ISSUER: &str = "https://accounts.google.com";

/// Who Google says the person is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// Google's stable account ID (`sub`).
    pub subject: String,
    pub email: String,
    pub name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum GoogleError {
    #[error("Google sign-in is not configured")]
    NotConfigured,
    #[error("Google could not be reached: {0}")]
    Unreachable(String),
    #[error("Google rejected the sign-in: {0}")]
    Rejected(String),
    #[error("the Google account has no verified email address")]
    UnverifiedEmail,
}

type Client = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

struct Connected {
    client: Client,
    http: reqwest::Client,
}

/// Talks to Google. Discovery happens on first use, so the server starts
/// without network access.
pub struct GoogleProvider {
    config: Option<GoogleConfig>,
    redirect_uri: String,
    connected: OnceCell<Connected>,
}

/// Where to send the browser, and what to remember until it comes back.
pub struct Authorization {
    pub url: Url,
    pub nonce: String,
    pub pkce_verifier: String,
}

impl GoogleProvider {
    pub fn new(config: Option<GoogleConfig>, public_url: &str) -> Self {
        Self {
            config,
            redirect_uri: format!("{public_url}/v1/auth/google/callback"),
            connected: OnceCell::new(),
        }
    }

    async fn connected(&self) -> Result<&Connected, GoogleError> {
        let config = self.config.as_ref().ok_or(GoogleError::NotConfigured)?;
        self.connected
            .get_or_try_init(|| async {
                let http = reqwest::ClientBuilder::new()
                    // Following a redirect could be turned into request forgery.
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .map_err(|error| GoogleError::Unreachable(error.to_string()))?;
                let issuer = IssuerUrl::new(ISSUER.to_owned())
                    .map_err(|error| GoogleError::Unreachable(error.to_string()))?;
                let metadata = CoreProviderMetadata::discover_async(issuer, &http)
                    .await
                    .map_err(|error| GoogleError::Unreachable(error.to_string()))?;
                let redirect = RedirectUrl::new(self.redirect_uri.clone())
                    .map_err(|error| GoogleError::Unreachable(error.to_string()))?;
                let client = CoreClient::from_provider_metadata(
                    metadata,
                    ClientId::new(config.client_id.clone()),
                    Some(ClientSecret::new(config.client_secret.clone())),
                )
                .set_redirect_uri(redirect);
                Ok(Connected { client, http })
            })
            .await
    }

    /// Builds the URL to send the browser to. `state` ties the callback to
    /// the flow stored by the caller. A step-up forces Google to ask for the
    /// password again.
    pub async fn authorize(
        &self,
        state: &str,
        step_up: bool,
    ) -> Result<Authorization, GoogleError> {
        let connected = self.connected().await?;
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let state = state.to_owned();
        let mut request = connected
            .client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                move || CsrfToken::new(state.clone()),
                Nonce::new_random,
            )
            .set_pkce_challenge(challenge)
            .add_scope(Scope::new("openid".into()))
            .add_scope(Scope::new("email".into()))
            .add_scope(Scope::new("profile".into()));
        if step_up {
            request = request.add_extra_param("prompt", "login");
        } else {
            request = request.add_extra_param("prompt", "select_account");
        }
        let (url, _state, nonce) = request.url();
        Ok(Authorization {
            url,
            nonce: nonce.secret().clone(),
            pkce_verifier: verifier.secret().clone(),
        })
    }

    /// Trades the code for an ID token and returns the verified identity.
    pub async fn identify(
        &self,
        code: &str,
        pkce_verifier: &str,
        nonce: &str,
    ) -> Result<Identity, GoogleError> {
        let connected = self.connected().await?;
        let response = connected
            .client
            .exchange_code(AuthorizationCode::new(code.to_owned()))
            .map_err(|error| GoogleError::Rejected(error.to_string()))?
            .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier.to_owned()))
            .request_async(&connected.http)
            .await
            .map_err(|error| GoogleError::Rejected(error.to_string()))?;
        let id_token = response
            .id_token()
            .ok_or_else(|| GoogleError::Rejected("no ID token".into()))?;
        let claims = id_token
            .claims(
                &connected.client.id_token_verifier(),
                &Nonce::new(nonce.to_owned()),
            )
            .map_err(|error| GoogleError::Rejected(error.to_string()))?;
        let email = claims
            .email()
            .ok_or(GoogleError::UnverifiedEmail)?
            .to_string();
        if claims.email_verified() != Some(true) {
            return Err(GoogleError::UnverifiedEmail);
        }
        let name = claims
            .name()
            .and_then(|name| name.get(None))
            .map(|name| name.to_string())
            .unwrap_or_default();
        Ok(Identity {
            subject: claims.subject().to_string(),
            email,
            name,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn without_configuration_google_is_unavailable() {
        let provider = GoogleProvider::new(None, "http://127.0.0.1:8080");
        assert!(matches!(
            provider.authorize("state", false).await,
            Err(GoogleError::NotConfigured)
        ));
        assert!(matches!(
            provider.identify("code", "verifier", "nonce").await,
            Err(GoogleError::NotConfigured)
        ));
    }
}
