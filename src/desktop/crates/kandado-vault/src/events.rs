//! The connection that tells this computer something changed on the server
//! (`GET /v1/events`, a WebSocket). It carries no data: a message means "pull".
//! If it drops, the scheduler reconnects with a delay and also polls, so a
//! missed message only makes a change arrive a little later.

use std::sync::Once;

use futures_util::StreamExt;
use kandado_protocol::SyncEvent;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Error as WsError, Message, client::IntoClientRequest},
};
use url::Url;

use crate::ApiError;

/// An open connection. Call [`EventStream::next`] in a loop.
pub struct EventStream {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

pub(crate) enum ConnectError {
    Unauthorized,
    Failed(ApiError),
}

impl ConnectError {
    pub(crate) fn into_api(self) -> ApiError {
        match self {
            Self::Unauthorized => ApiError::SignedOut,
            Self::Failed(error) => error,
        }
    }
}

/// TLS needs one crypto provider chosen for the whole process, because more
/// than one is compiled in. Chosen once, the first time a connection opens.
fn choose_tls_provider() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

/// `https` becomes `wss`, `http` becomes `ws`.
fn websocket_address(address: &Url) -> Result<Url, ApiError> {
    let mut address = address.clone();
    let scheme = match address.scheme() {
        "https" => "wss",
        "http" => "ws",
        other => {
            return Err(ApiError::Unexpected(format!(
                "cannot open events over {other}"
            )));
        }
    };
    address
        .set_scheme(scheme)
        .map_err(|()| ApiError::Unexpected("bad events address".into()))?;
    Ok(address)
}

pub(crate) async fn connect(address: &Url, access: &str) -> Result<EventStream, ConnectError> {
    choose_tls_provider();
    let failed = |error: String| ConnectError::Failed(ApiError::Offline(error));
    let address = websocket_address(address).map_err(ConnectError::Failed)?;
    let mut request = address
        .as_str()
        .into_client_request()
        .map_err(|error| failed(error.to_string()))?;
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {access}")
            .parse()
            .map_err(|_| failed("bad token".into()))?,
    );
    match connect_async(request).await {
        Ok((socket, _)) => Ok(EventStream { socket }),
        Err(WsError::Http(response)) if response.status().as_u16() == 401 => {
            Err(ConnectError::Unauthorized)
        }
        Err(error) => Err(failed(error.to_string())),
    }
}

impl EventStream {
    /// Waits for the next "changed". `None` when the connection closed.
    pub async fn next(&mut self) -> Option<()> {
        loop {
            match self.socket.next().await? {
                Ok(Message::Text(text)) => {
                    if serde_json::from_str::<SyncEvent>(&text).is_ok() {
                        return Some(());
                    }
                }
                Ok(Message::Close(_)) | Err(_) => return None,
                // Pings are answered by the library; anything else is ignored.
                Ok(_) => {}
            }
        }
    }
}
