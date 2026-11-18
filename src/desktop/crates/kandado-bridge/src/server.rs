//! The pipe server inside the desktop app.
//!
//! Each native host that connects (one per browser) gets its own connection.
//! Requests are answered by a [`Handler`] the app provides; the app's lock and
//! unlock are sent to every connection as events.

use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{broadcast, mpsc},
};
use uuid::Uuid;

use crate::protocol::{
    Event, EventFrame, FrameError, Hello, Refusal, Request, RequestBody, Response, ResponseBody,
    read_frame, write_frame,
};

/// What the app answers. Called on the bridge's runtime: keep the sync calls quick.
pub trait Handler: Send + Sync + 'static {
    fn status(&self) -> ResponseBody;
    /// Waits for the person to confirm the code in the app (D-45).
    fn pair(
        &self,
        origin: String,
        pairing_id: Uuid,
        public_key: [u8; 32],
        browser: String,
    ) -> Pin<Box<dyn Future<Output = ResponseBody> + Send>>;
    fn unlock(&self, pairing_id: Uuid) -> ResponseBody;
    fn unpair(&self, pairing_id: Uuid) -> ResponseBody;
}

/// How long a native host has to say which browser it serves.
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

/// Serves one connection: a hello, then requests and events until either side closes.
pub async fn serve_connection<S>(
    stream: S,
    handler: Arc<dyn Handler>,
    mut events: broadcast::Receiver<Event>,
) -> Result<(), FrameError>
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (mut reader, mut writer) = tokio::io::split(stream);
    let hello = tokio::time::timeout(HELLO_TIMEOUT, read_frame(&mut reader))
        .await
        .map_err(|_| FrameError::Closed)??;
    let Hello::Connect { origin } = serde_json::from_slice(&hello)?;

    // Answers come back from their own tasks (a pairing waits for the person),
    // so one writer task sends both answers and events.
    let (out, mut outgoing) = mpsc::channel::<Vec<u8>>(32);
    let writer_task = tokio::spawn(async move {
        loop {
            let frame = tokio::select! {
                frame = outgoing.recv() => match frame {
                    Some(frame) => frame,
                    None => break,
                },
                event = events.recv() => match event {
                    Ok(event) => match serde_json::to_vec(&EventFrame::Event { event }) {
                        Ok(frame) => frame,
                        Err(_) => continue,
                    },
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                },
            };
            if write_frame(&mut writer, &frame).await.is_err() {
                break;
            }
        }
    });

    loop {
        let frame = match read_frame(&mut reader).await {
            Ok(frame) => frame,
            Err(FrameError::Closed) => break,
            Err(error) => {
                writer_task.abort();
                return Err(error);
            }
        };
        let request: Request = match serde_json::from_slice(&frame) {
            Ok(request) => request,
            Err(_) => {
                let refused = Response {
                    id: 0,
                    body: ResponseBody::Refused {
                        reason: Refusal::Invalid,
                    },
                };
                let _ = out.send(serde_json::to_vec(&refused)?).await;
                continue;
            }
        };
        let handler = handler.clone();
        let out = out.clone();
        let origin = origin.clone();
        tokio::spawn(async move {
            let body = answer(&*handler, origin, request.body).await;
            if let Ok(frame) = serde_json::to_vec(&Response {
                id: request.id,
                body,
            }) {
                let _ = out.send(frame).await;
            }
        });
    }
    drop(out);
    let _ = writer_task.await;
    Ok(())
}

async fn answer(handler: &dyn Handler, origin: String, body: RequestBody) -> ResponseBody {
    match body {
        RequestBody::Status => handler.status(),
        RequestBody::Pair {
            pairing_id,
            public_key,
            browser,
        } => {
            let Some(key) = decode_key(&public_key) else {
                return ResponseBody::Refused {
                    reason: Refusal::Invalid,
                };
            };
            let browser: String = browser.chars().take(40).collect();
            handler.pair(origin, pairing_id, key, browser).await
        }
        RequestBody::Unlock { pairing_id } => handler.unlock(pairing_id),
        RequestBody::Unpair { pairing_id } => handler.unpair(pairing_id),
    }
}

/// A 32-byte public key from standard base64.
pub fn decode_key(text: &str) -> Option<[u8; 32]> {
    let bytes = base64_decode(text)?;
    bytes.try_into().ok()
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    STANDARD.decode(text.as_bytes()).ok()
}

/// The running pipe server. Dropping it stops listening.
pub struct Bridge {
    events: broadcast::Sender<Event>,
    task: tokio::task::JoinHandle<()>,
}

impl Bridge {
    /// Starts serving the current user's pipe on `runtime`.
    #[cfg(windows)]
    pub fn start(
        runtime: &tokio::runtime::Handle,
        handler: Arc<dyn Handler>,
    ) -> Result<Self, crate::pipe::PipeError> {
        let name = crate::pipe::pipe_name()?;
        Self::start_named(runtime, name, handler)
    }

    /// Starts serving a pipe of the given name (tests use their own).
    #[cfg(windows)]
    pub fn start_named(
        runtime: &tokio::runtime::Handle,
        name: String,
        handler: Arc<dyn Handler>,
    ) -> Result<Self, crate::pipe::PipeError> {
        let _guard = runtime.enter();
        let first = crate::pipe::create_server(&name, true)?;
        let (events, _) = broadcast::channel(16);
        let sender = events.clone();
        let task = runtime.spawn(async move {
            let mut server = first;
            loop {
                if server.connect().await.is_err() {
                    match crate::pipe::create_server(&name, false) {
                        Ok(next) => server = next,
                        Err(error) => {
                            tracing::warn!(%error, "the desktop link stopped");
                            return;
                        }
                    }
                    continue;
                }
                let connected = server;
                server = match crate::pipe::create_server(&name, false) {
                    Ok(next) => next,
                    Err(error) => {
                        tracing::warn!(%error, "the desktop link stopped");
                        return;
                    }
                };
                let handler = handler.clone();
                let events = sender.subscribe();
                tokio::spawn(async move {
                    if let Err(error) = serve_connection(connected, handler, events).await {
                        tracing::debug!(%error, "a browser link closed");
                    }
                });
            }
        });
        Ok(Self { events, task })
    }

    /// Tells every connected extension that the app locked or unlocked.
    pub fn notify(&self, event: Event) {
        let _ = self.events.send(event);
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::protocol::{AppState, read_message, write_message};

    #[derive(Default)]
    struct Fake {
        paired: Mutex<Vec<(String, Uuid, String)>>,
    }

    impl Handler for Fake {
        fn status(&self) -> ResponseBody {
            ResponseBody::Status {
                version: "0.1.0".into(),
                state: AppState::Unlocked,
                user_id: None,
                unlock_enabled: true,
            }
        }
        fn pair(
            &self,
            origin: String,
            pairing_id: Uuid,
            _public_key: [u8; 32],
            browser: String,
        ) -> Pin<Box<dyn Future<Output = ResponseBody> + Send>> {
            self.paired
                .lock()
                .unwrap()
                .push((origin, pairing_id, browser));
            Box::pin(async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                ResponseBody::Paired
            })
        }
        fn unlock(&self, _pairing_id: Uuid) -> ResponseBody {
            ResponseBody::Refused {
                reason: Refusal::NotPaired,
            }
        }
        fn unpair(&self, _pairing_id: Uuid) -> ResponseBody {
            ResponseBody::Done
        }
    }

    #[tokio::test]
    async fn requests_are_answered_by_id_and_events_reach_the_extension() {
        let (client, server) = tokio::io::duplex(4096);
        let fake = Arc::new(Fake::default());
        let (events, receiver) = broadcast::channel(4);
        let serving = tokio::spawn(serve_connection(server, fake.clone(), receiver));
        let (mut reader, mut writer) = tokio::io::split(client);

        write_message(
            &mut writer,
            &Hello::Connect {
                origin: "chrome-extension://abc/".into(),
            },
        )
        .await
        .unwrap();
        // A slow pairing, then a quick status: answers arrive as they finish.
        let key = {
            use base64::{Engine, engine::general_purpose::STANDARD};
            STANDARD.encode([5u8; 32])
        };
        write_frame(
            &mut writer,
            format!(r#"{{"id":1,"type":"pair","pairing_id":"00000000-0000-0000-0000-000000000009","public_key":"{key}","browser":"Edge"}}"#).as_bytes(),
        )
        .await
        .unwrap();
        write_frame(&mut writer, br#"{"id":2,"type":"status"}"#)
            .await
            .unwrap();
        let first: Response = read_message(&mut reader).await.unwrap();
        assert_eq!(first.id, 2);
        let second: Response = read_message(&mut reader).await.unwrap();
        assert_eq!(
            second,
            Response {
                id: 1,
                body: ResponseBody::Paired
            }
        );
        assert_eq!(
            fake.paired.lock().unwrap()[0],
            (
                "chrome-extension://abc/".to_owned(),
                Uuid::from_u128(9),
                "Edge".to_owned()
            )
        );

        events.send(Event::Locked).unwrap();
        let event: EventFrame = read_message(&mut reader).await.unwrap();
        assert_eq!(
            event,
            EventFrame::Event {
                event: Event::Locked
            }
        );

        // A bad key and nonsense are refused, not fatal.
        write_frame(
            &mut writer,
            br#"{"id":3,"type":"pair","pairing_id":"00000000-0000-0000-0000-000000000009","public_key":"short","browser":"x"}"#,
        )
        .await
        .unwrap();
        let refused: Response = read_message(&mut reader).await.unwrap();
        assert_eq!(
            refused.body,
            ResponseBody::Refused {
                reason: Refusal::Invalid
            }
        );
        write_frame(&mut writer, b"not json").await.unwrap();
        let refused: Response = read_message(&mut reader).await.unwrap();
        assert_eq!(refused.id, 0);

        drop(writer);
        drop(reader);
        serving.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn a_connection_must_start_with_hello() {
        let (client, server) = tokio::io::duplex(4096);
        let (_events, receiver) = broadcast::channel(4);
        let serving = tokio::spawn(serve_connection(
            server,
            Arc::new(Fake::default()),
            receiver,
        ));
        let (_reader, mut writer) = tokio::io::split(client);
        write_frame(&mut writer, br#"{"id":1,"type":"status"}"#)
            .await
            .unwrap();
        assert!(serving.await.unwrap().is_err());
    }
}
