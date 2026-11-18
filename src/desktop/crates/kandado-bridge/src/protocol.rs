//! What travels between the extension and the desktop app.
//!
//! Every message is JSON, framed the way native messaging frames it: a 32-bit
//! little-endian length, then the bytes. The native host relays frames between
//! the browser (stdin and stdout) and the desktop app (the named pipe) without
//! changing them, after one `connect` frame of its own that names the browser.
//!
//! The bridge never carries item data: the extension has its own copy of the
//! vault. It carries the app's state, a pairing request, and the user key sealed
//! to a paired extension (`kandado_crypto::pairing`).

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

/// Native messaging allows 1 MiB from the host to the browser. Nothing here comes close.
pub const MAX_FRAME: usize = 1024 * 1024;

/// The native messaging host's name, as browsers know it.
pub const HOST_NAME: &str = "app.kandado.native";

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("the other side closed the connection")]
    Closed,
    #[error("a frame of {0} bytes is larger than allowed")]
    TooLarge(usize),
    #[error("the frame is not a message: {0}")]
    Malformed(#[from] serde_json::Error),
    #[error(transparent)]
    Io(std::io::Error),
}

/// Reads one frame. `Closed` at a clean end of stream.
pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Vec<u8>, FrameError> {
    let mut length = [0u8; 4];
    match reader.read_exact(&mut length).await {
        Ok(_) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::BrokenPipe
            ) =>
        {
            return Err(FrameError::Closed);
        }
        Err(error) => return Err(FrameError::Io(error)),
    }
    let length = u32::from_le_bytes(length) as usize;
    if length > MAX_FRAME {
        return Err(FrameError::TooLarge(length));
    }
    let mut bytes = vec![0u8; length];
    reader
        .read_exact(&mut bytes)
        .await
        .map_err(FrameError::Io)?;
    Ok(bytes)
}

pub async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    bytes: &[u8],
) -> Result<(), FrameError> {
    if bytes.len() > MAX_FRAME {
        return Err(FrameError::TooLarge(bytes.len()));
    }
    let length = u32::try_from(bytes.len()).map_err(|_| FrameError::TooLarge(bytes.len()))?;
    writer
        .write_all(&length.to_le_bytes())
        .await
        .map_err(FrameError::Io)?;
    writer.write_all(bytes).await.map_err(FrameError::Io)?;
    writer.flush().await.map_err(FrameError::Io)
}

pub async fn read_message<R, T>(reader: &mut R) -> Result<T, FrameError>
where
    R: AsyncRead + Unpin,
    T: for<'de> Deserialize<'de>,
{
    Ok(serde_json::from_slice(&read_frame(reader).await?)?)
}

pub async fn write_message<W: AsyncWrite + Unpin, T: Serialize>(
    writer: &mut W,
    message: &T,
) -> Result<(), FrameError> {
    write_frame(writer, &serde_json::to_vec(message)?).await
}

/// The first frame on the pipe, from the native host: which browser is on the
/// other end, as the host learned it from the browser's arguments. Shown to the
/// person; it decides nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Hello {
    Connect {
        /// `chrome-extension://{id}/`, or the Firefox add-on ID.
        origin: String,
    },
}

/// A request from the extension. `id` is echoed in the answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    #[serde(flatten)]
    pub body: RequestBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RequestBody {
    /// Is the app there, and is it unlocked?
    Status,
    /// Pair this extension. The person confirms the code on both sides.
    Pair {
        pairing_id: Uuid,
        /// The extension's X25519 public key, base64.
        public_key: String,
        /// What the extension calls its browser ("Edge", "Chrome", "Firefox").
        browser: String,
    },
    /// The user key, sealed to this pairing.
    Unlock { pairing_id: Uuid },
    /// Forget this pairing.
    Unpair { pairing_id: Uuid },
}

/// The app's state, without anything secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppState {
    SignedOut,
    Locked,
    Unlocked,
}

/// Why an unlock or a pairing was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// The app is locked or signed out.
    Locked,
    /// No such pairing on this computer, for this account.
    NotPaired,
    /// "Unlock the browser extension with this app" is off (D-45).
    TurnedOff,
    /// The person said no, or did not answer in time.
    Declined,
    /// Malformed request.
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    #[serde(flatten)]
    pub body: ResponseBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseBody {
    Status {
        version: String,
        state: AppState,
        /// The signed-in account, so the extension can tell it is the same one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user_id: Option<Uuid>,
        /// Whether "Unlock the browser extension with this app" is on.
        unlock_enabled: bool,
    },
    Paired,
    /// The user key sealed to the pairing's public key, base64.
    Unlocked {
        sealed: String,
    },
    Done,
    Refused {
        reason: Refusal,
    },
}

/// Something that happened in the app, sent to every connected extension:
/// `{"type":"event","event":"locked"}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventFrame {
    Event { event: Event },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    Locked,
    Unlocked,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format() {
        let request: Request = serde_json::from_str(
            r#"{"id":3,"type":"unlock","pairing_id":"00000000-0000-0000-0000-000000000001"}"#,
        )
        .unwrap();
        assert_eq!(
            request,
            Request {
                id: 3,
                body: RequestBody::Unlock {
                    pairing_id: Uuid::from_u128(1)
                }
            }
        );
        let answer = Response {
            id: 3,
            body: ResponseBody::Refused {
                reason: Refusal::Locked,
            },
        };
        assert_eq!(
            serde_json::to_string(&answer).unwrap(),
            r#"{"id":3,"type":"refused","reason":"locked"}"#
        );
        let event = EventFrame::Event {
            event: Event::Locked,
        };
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"type":"event","event":"locked"}"#
        );
        let status = Response {
            id: 1,
            body: ResponseBody::Status {
                version: "0.1.0".into(),
                state: AppState::Unlocked,
                user_id: None,
                unlock_enabled: true,
            },
        };
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            r#"{"id":1,"type":"status","version":"0.1.0","state":"unlocked","unlock_enabled":true}"#
        );
        let hello = Hello::Connect {
            origin: "chrome-extension://abc/".into(),
        };
        assert_eq!(
            serde_json::to_string(&hello).unwrap(),
            r#"{"type":"connect","origin":"chrome-extension://abc/"}"#
        );
    }

    #[tokio::test]
    async fn frames_round_trip_and_refuse_oversize() {
        let (mut a, mut b) = tokio::io::duplex(64);
        write_frame(&mut a, b"{\"x\":1}").await.unwrap();
        assert_eq!(read_frame(&mut b).await.unwrap(), b"{\"x\":1}");

        let (mut a, mut b) = tokio::io::duplex(64);
        a.write_all(&((MAX_FRAME as u32) + 1).to_le_bytes())
            .await
            .unwrap();
        assert!(matches!(
            read_frame(&mut b).await,
            Err(FrameError::TooLarge(_))
        ));

        let (a, mut b) = tokio::io::duplex(64);
        drop(a);
        assert!(matches!(read_frame(&mut b).await, Err(FrameError::Closed)));
    }
}
