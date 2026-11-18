//! Google sign-in from the desktop: open the browser at the server, listen on
//! a loopback port for the redirect back, and return the single-use code.
//!
//! The code is useless without the `verifier` that never left this process
//! (PKCE-style handoff, see `docs/architecture/security.md`).

use std::time::Duration;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use kandado_protocol::FlowPurpose;
use kandado_vault::Api;
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{Instant, timeout_at},
};
use url::Url;

/// How long the person has to finish signing in at Google.
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("sign-in was cancelled")]
    Denied,
    #[error("Google could not sign you in")]
    Failed,
    #[error("that is not the Google account this one is signed in with")]
    WrongAccount,
    #[error("sign-in took too long")]
    TimedOut,
    #[error("sign-in could not start: {0}")]
    Io(#[from] std::io::Error),
}

/// A sign-in that has been started and is waiting for the browser.
pub struct PendingSignIn {
    listener: TcpListener,
    pub purpose: FlowPurpose,
    /// Open this in the system browser.
    pub url: Url,
    verifier: String,
}

impl PendingSignIn {
    /// Binds a loopback port and builds the address to open. For a step-up,
    /// pass the current access token so the server knows which session is
    /// being confirmed.
    pub async fn start(
        api: &Api,
        purpose: FlowPurpose,
        access_token: Option<&str>,
    ) -> Result<Self, AuthError> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();

        let mut random = [0u8; 32];
        kandado_crypto::random::fill(&mut random)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let verifier = URL_SAFE_NO_PAD.encode(random);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));

        let redirect = format!("http://127.0.0.1:{port}/callback");
        let url = api.google_start_url(&challenge, &redirect, purpose, access_token);
        Ok(Self {
            purpose,
            listener,
            url,
            verifier,
        })
    }

    /// The secret that proves this process started the sign-in.
    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    /// Waits for the browser to come back, up to [`SIGN_IN_TIMEOUT`], and
    /// returns the code plus the verifier to exchange it with.
    pub async fn wait(self) -> Result<(String, String), AuthError> {
        self.wait_for(SIGN_IN_TIMEOUT).await
    }

    async fn wait_for(self, limit: Duration) -> Result<(String, String), AuthError> {
        let port = self.listener.local_addr()?.port();
        let deadline = Instant::now() + limit;
        loop {
            let (stream, _) = timeout_at(deadline, self.listener.accept())
                .await
                .map_err(|_| AuthError::TimedOut)??;
            // A stray request (favicon, a scanner) must not end the wait.
            match timeout_at(deadline, answer(stream, port)).await {
                Err(_) => return Err(AuthError::TimedOut),
                Ok(None) => continue,
                Ok(Some(Ok(code))) => return Ok((code, self.verifier)),
                Ok(Some(Err(error))) => return Err(error),
            }
        }
    }
}

const PAGE: &str = "<!doctype html><meta charset=utf-8><title>Kandado</title>\
<body style=\"font-family:system-ui;margin:4rem auto;max-width:28rem;text-align:center\">\
<h1>You can close this tab</h1><p>Go back to Kandado to continue.</p></body>";

/// Reads one request. Returns `None` if it was not the callback.
async fn answer(mut stream: TcpStream, port: u16) -> Option<Result<String, AuthError>> {
    let mut buffer = vec![0u8; 8192];
    let mut filled = 0;
    // Read until the end of the headers, or give up on a slow client.
    let read = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let count = stream.read(&mut buffer[filled..]).await.ok()?;
            if count == 0 {
                return None;
            }
            filled += count;
            if buffer[..filled].windows(4).any(|w| w == b"\r\n\r\n") || filled == buffer.len() {
                return Some(());
            }
        }
    })
    .await;
    if !matches!(read, Ok(Some(()))) {
        return None;
    }
    let request = String::from_utf8_lossy(&buffer[..filled]).into_owned();
    let result = parse_callback(&request, port);
    let (status, body) = match &result {
        Some(_) => ("200 OK", PAGE),
        None => ("404 Not Found", ""),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
    result
}

/// Pulls the code, or the reason it failed, out of the callback request.
/// Anything else, including a wrong Host header (a DNS-rebinding attempt),
/// is ignored.
fn parse_callback(request: &str, port: u16) -> Option<Result<String, AuthError>> {
    let mut lines = request.split("\r\n");
    let mut first = lines.next()?.split(' ');
    if first.next()? != "GET" {
        return None;
    }
    let target = first.next()?;
    let host = lines
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("host").then(|| value.trim())
        })
        .unwrap_or_default();
    if host != format!("127.0.0.1:{port}") {
        return None;
    }
    let url = Url::parse(&format!("http://127.0.0.1:{port}{target}")).ok()?;
    if url.path() != "/callback" {
        return None;
    }
    let mut code = None;
    let mut error = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            _ => {}
        }
    }
    Some(match (code, error.as_deref()) {
        (Some(code), None) if !code.is_empty() => Ok(code),
        (_, Some("access_denied")) => Err(AuthError::Denied),
        (_, Some("wrong_account")) => Err(AuthError::WrongAccount),
        _ => Err(AuthError::Failed),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn browser(port: u16, request: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).await.unwrap();
        reply
    }

    fn get(port: u16, target: &str) -> String {
        format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n")
    }

    async fn pending() -> (PendingSignIn, u16) {
        let api = Api::new("https://api.example.test", |_| {}).unwrap();
        let pending = PendingSignIn::start(&api, FlowPurpose::SignIn, None)
            .await
            .unwrap();
        let port = pending.listener.local_addr().unwrap().port();
        (pending, port)
    }

    #[tokio::test]
    async fn the_start_url_carries_a_challenge_for_the_verifier() {
        let (pending, port) = pending().await;
        let pairs: std::collections::HashMap<_, _> = pending.url.query_pairs().collect();
        assert_eq!(
            pairs["redirect"],
            format!("http://127.0.0.1:{port}/callback")
        );
        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(pending.verifier().as_bytes()));
        assert_eq!(pairs["challenge"], expected);
        assert_eq!(pairs["client"], "desktop");
    }

    #[tokio::test]
    async fn the_code_comes_back_and_the_browser_gets_a_page() {
        let (pending, port) = pending().await;
        let verifier = pending.verifier().to_owned();
        let waiting = tokio::spawn(pending.wait());
        let reply = browser(port, &get(port, "/callback?code=abc123")).await;
        assert!(reply.starts_with("HTTP/1.1 200 OK"));
        assert!(reply.contains("You can close this tab"));
        let (code, returned) = waiting.await.unwrap().unwrap();
        assert_eq!(code, "abc123");
        assert_eq!(returned, verifier);
    }

    #[tokio::test]
    async fn stray_requests_do_not_end_the_wait() {
        let (pending, port) = pending().await;
        let waiting = tokio::spawn(pending.wait());
        let favicon = browser(port, &get(port, "/favicon.ico")).await;
        assert!(favicon.starts_with("HTTP/1.1 404"));
        // A request that names another host (DNS rebinding) is ignored too.
        let rebinding = browser(
            port,
            &format!("GET /callback?code=evil HTTP/1.1\r\nHost: evil.test:{port}\r\n\r\n"),
        )
        .await;
        assert!(rebinding.starts_with("HTTP/1.1 404"));
        browser(port, &get(port, "/callback?code=real")).await;
        assert_eq!(waiting.await.unwrap().unwrap().0, "real");
    }

    #[tokio::test]
    async fn errors_from_the_server_are_told_apart() {
        for (query, expected) in [
            ("error=access_denied", "cancelled"),
            ("error=wrong_account", "not the Google account"),
            ("error=sign_in_failed", "could not sign you in"),
            ("", "could not sign you in"),
        ] {
            let (pending, port) = pending().await;
            let waiting = tokio::spawn(pending.wait());
            browser(port, &get(port, &format!("/callback?{query}"))).await;
            let error = waiting.await.unwrap().unwrap_err().to_string();
            assert!(error.contains(expected), "{query}: {error}");
        }
    }

    #[tokio::test]
    async fn waiting_gives_up_after_the_limit() {
        let (pending, _) = pending().await;
        let error = pending
            .wait_for(Duration::from_millis(50))
            .await
            .unwrap_err();
        assert!(matches!(error, AuthError::TimedOut));
    }
}
