//! Realtime: `GET /v1/events`, a WebSocket that tells a device its account
//! changed so it pulls at once instead of waiting for its next poll.
//!
//! A write commits and, in the same transaction, calls `pg_notify` with the
//! user's id. One listener per server process receives those and hands them
//! to the [`Hub`], which wakes the sockets of that user. Postgres delivers
//! notifications to every server instance, so this works with more than one.
//!
//! The message carries no data, only "changed". The device pulls, which is
//! authorised like any other pull.

use std::time::Duration;

use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
};
use kandado_protocol::SyncEvent;
use sqlx::{PgPool, postgres::PgListener};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{auth::Session, db, http::AppState};

/// How often a socket is pinged, and a revoked device is noticed.
const PING_EVERY: Duration = Duration::from_secs(30);

/// Wakes the sockets of a user.
#[derive(Clone)]
pub struct Hub {
    tx: broadcast::Sender<Uuid>,
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

impl Hub {
    pub fn new() -> Self {
        Self {
            tx: broadcast::channel(256).0,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Uuid> {
        self.tx.subscribe()
    }

    /// Tells every socket of `user` that something changed.
    pub fn notify(&self, user: Uuid) {
        // No receivers is normal: nobody is connected.
        let _ = self.tx.send(user);
    }

    /// Listens to Postgres for the rest of the process's life.
    pub fn listen(&self, db: PgPool) {
        let hub = self.clone();
        tokio::spawn(async move {
            loop {
                let mut listener = match PgListener::connect_with(&db).await {
                    Ok(listener) => listener,
                    Err(error) => {
                        tracing::warn!(%error, "the event listener could not connect");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                };
                if let Err(error) = listener.listen(db::sync::CHANNEL).await {
                    tracing::warn!(%error, "the event listener could not subscribe");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                tracing::info!("listening for changes");
                loop {
                    match listener.recv().await {
                        Ok(notification) => {
                            if let Ok(user) = notification.payload().parse() {
                                hub.notify(user);
                            }
                        }
                        Err(error) => {
                            // The connection dropped. Devices also poll, so a
                            // missed notification only delays them.
                            tracing::warn!(%error, "the event listener lost its connection");
                            break;
                        }
                    }
                }
            }
        });
    }
}

/// `GET /v1/events`
pub async fn events(
    State(state): State<AppState>,
    session: Session,
    ws: WebSocketUpgrade,
) -> Response {
    let receiver = state.hub.subscribe();
    ws.on_upgrade(move |socket| serve(socket, state, receiver, session))
}

async fn send_changed(socket: &mut WebSocket) -> bool {
    let text = serde_json::to_string(&SyncEvent::Changed).expect("a fixed message serialises");
    socket.send(Message::Text(text.into())).await.is_ok()
}

async fn serve(
    mut socket: WebSocket,
    state: AppState,
    mut receiver: broadcast::Receiver<Uuid>,
    session: Session,
) {
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;
    loop {
        tokio::select! {
            woken = receiver.recv() => match woken {
                Ok(user) if user == session.user_id => {
                    if !send_changed(&mut socket).await {
                        break;
                    }
                }
                Ok(_) => {}
                // Missed some wake-ups: say so, the device pulls everything.
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    if !send_changed(&mut socket).await {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                None | Some(Err(_) | Ok(Message::Close(_))) => break,
                Some(Ok(_)) => {}
            },
            _ = ping.tick() => {
                // A device revoked from another one stops being served now.
                let active = db::devices::touch_active(&state.db, session.user_id, session.device_id)
                    .await
                    .unwrap_or(true);
                if !active || socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
        }
    }
}
