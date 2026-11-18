//! What `kandado-native-host` does: relay frames between the browser (stdin and
//! stdout) and the desktop app's pipe, without reading what they say.

use tokio::io::{AsyncRead, AsyncWrite};

use crate::protocol::{FrameError, Hello, read_frame, write_frame, write_message};

/// Which extension started the host, from the arguments the browser passed.
/// Chrome and Edge pass `chrome-extension://{id}/` (then `--parent-window=…` on
/// Windows); Firefox passes the manifest's path, then the add-on ID.
pub fn origin_from_args(args: &[String]) -> Option<String> {
    if let Some(origin) = args
        .iter()
        .find(|arg| arg.starts_with("chrome-extension://"))
    {
        return Some(origin.clone());
    }
    match args {
        [_, manifest, id, ..] if manifest.ends_with(".json") && !id.is_empty() => Some(id.clone()),
        _ => None,
    }
}

/// The one frame the host sends when the desktop app is not running, before it
/// exits: `{"type":"unavailable"}`.
pub const UNAVAILABLE: &[u8] = br#"{"type":"unavailable"}"#;

/// Relays until either side closes.
pub async fn relay<BI, BO, P>(
    mut browser_in: BI,
    mut browser_out: BO,
    pipe: P,
    origin: String,
) -> Result<(), FrameError>
where
    BI: AsyncRead + Unpin,
    BO: AsyncWrite + Unpin,
    P: AsyncRead + AsyncWrite,
{
    let (mut pipe_in, mut pipe_out) = tokio::io::split(pipe);
    write_message(&mut pipe_out, &Hello::Connect { origin }).await?;
    let to_app = async {
        loop {
            match read_frame(&mut browser_in).await {
                Ok(frame) => write_frame(&mut pipe_out, &frame).await?,
                Err(FrameError::Closed) => return Ok(()),
                Err(error) => return Err(error),
            }
        }
    };
    let to_browser = async {
        loop {
            match read_frame(&mut pipe_in).await {
                Ok(frame) => write_frame(&mut browser_out, &frame).await?,
                Err(FrameError::Closed) => return Ok(()),
                Err(error) => return Err(error),
            }
        }
    };
    tokio::select! {
        result = to_app => result,
        result = to_browser => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::read_message;

    #[test]
    fn origins_from_each_browser() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            origin_from_args(&args(&[
                "host.exe",
                "chrome-extension://abc/",
                "--parent-window=0"
            ])),
            Some("chrome-extension://abc/".into())
        );
        assert_eq!(
            origin_from_args(&args(&[
                "host.exe",
                r"C:\x\app.kandado.native.firefox.json",
                "kandado@kandado.app"
            ])),
            Some("kandado@kandado.app".into())
        );
        assert_eq!(origin_from_args(&args(&["host.exe"])), None);
    }

    #[tokio::test]
    async fn frames_pass_through_both_ways_after_the_hello() {
        let (browser, browser_host) = tokio::io::duplex(4096);
        let (app, app_host) = tokio::io::duplex(4096);
        let (host_in, host_out) = tokio::io::split(browser_host);
        let relaying = tokio::spawn(relay(
            host_in,
            host_out,
            app_host,
            "chrome-extension://abc/".into(),
        ));

        let (mut browser_read, mut browser_write) = tokio::io::split(browser);
        let (mut app_read, mut app_write) = tokio::io::split(app);
        let hello: Hello = read_message(&mut app_read).await.unwrap();
        assert_eq!(
            hello,
            Hello::Connect {
                origin: "chrome-extension://abc/".into()
            }
        );
        write_frame(&mut browser_write, br#"{"id":1,"type":"status"}"#)
            .await
            .unwrap();
        assert_eq!(
            read_frame(&mut app_read).await.unwrap(),
            br#"{"id":1,"type":"status"}"#
        );
        write_frame(&mut app_write, br#"{"type":"event","event":"locked"}"#)
            .await
            .unwrap();
        assert_eq!(
            read_frame(&mut browser_read).await.unwrap(),
            br#"{"type":"event","event":"locked"}"#
        );
        // The browser goes away (both halves: a split stream closes when both drop).
        drop(browser_write);
        drop(browser_read);
        relaying.await.unwrap().unwrap();
    }
}
