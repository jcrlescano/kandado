//! The real native host program against a real named pipe, as a browser would
//! drive it: frames on stdin, frames on stdout.
//!
//! The pipe has a test name (the debug host reads `KANDADO_BRIDGE_PIPE`), so a
//! running Kandado app is not disturbed. The manifests the host checks are
//! written next to the test build of the host and removed afterwards.
#![cfg(windows)]

use std::{
    future::Future,
    io,
    path::PathBuf,
    pin::Pin,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use kandado_bridge::{
    install::{self, AllowedExtensions, Registry},
    protocol::{
        AppState, Event, EventFrame, Refusal, Response, ResponseBody, read_message, write_frame,
    },
    server::{Bridge, Handler},
};
use tokio::process::Command;
use uuid::Uuid;

const ID: &str = "abcdefghijklmnopabcdefghijklmnop";

struct NoRegistry;
impl Registry for NoRegistry {
    fn set_default(&self, _: &str, _: &str) -> io::Result<()> {
        Ok(())
    }
    fn default_value(&self, _: &str) -> Option<String> {
        None
    }
    fn delete(&self, _: &str) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Default)]
struct App {
    origins: Mutex<Vec<String>>,
}

impl Handler for App {
    fn status(&self) -> ResponseBody {
        ResponseBody::Status {
            version: "0.1.0".into(),
            state: AppState::Locked,
            user_id: None,
            unlock_enabled: true,
        }
    }
    fn pair(
        &self,
        origin: String,
        _: Uuid,
        _: [u8; 32],
        _: String,
    ) -> Pin<Box<dyn Future<Output = ResponseBody> + Send>> {
        self.origins.lock().unwrap().push(origin);
        Box::pin(async { ResponseBody::Paired })
    }
    fn unlock(&self, _: Uuid) -> ResponseBody {
        ResponseBody::Refused {
            reason: Refusal::Locked,
        }
    }
    fn unpair(&self, _: Uuid) -> ResponseBody {
        ResponseBody::Done
    }
}

fn host() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_kandado-native-host"))
}

fn spawn(origin: &str, pipe: &str) -> tokio::process::Child {
    Command::new(host())
        .arg(origin)
        .arg("--parent-window=0")
        .env("KANDADO_BRIDGE_PIPE", pipe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("the host starts")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_browser_reaches_the_app_through_the_host_and_hears_its_events() {
    let allowed = AllowedExtensions {
        chromium_ids: vec![ID.into()],
        firefox_ids: vec![],
    };
    install::install(&host(), &allowed, &NoRegistry).unwrap();

    let pipe = format!(
        r"\\.\pipe\kandado-bridge-test-{}-{}",
        std::process::id(),
        Uuid::from_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    );
    let app = Arc::new(App::default());
    let bridge = Bridge::start_named(
        &tokio::runtime::Handle::current(),
        pipe.clone(),
        app.clone(),
    )
    .unwrap();

    let origin = format!("chrome-extension://{ID}/");
    let mut child = spawn(&origin, &pipe);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    write_frame(&mut stdin, br#"{"id":7,"type":"status"}"#)
        .await
        .unwrap();
    let status: Response = tokio::time::timeout(Duration::from_secs(10), read_message(&mut stdout))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.id, 7);
    assert!(matches!(
        status.body,
        ResponseBody::Status {
            state: AppState::Locked,
            ..
        }
    ));

    // The app learns which extension it is talking to from the host.
    write_frame(
        &mut stdin,
        br#"{"id":8,"type":"pair","pairing_id":"00000000-0000-0000-0000-000000000001","public_key":"BQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQU=","browser":"Edge"}"#,
    )
    .await
    .unwrap();
    let paired: Response = read_message(&mut stdout).await.unwrap();
    assert_eq!(paired.body, ResponseBody::Paired);
    assert_eq!(
        app.origins.lock().unwrap().as_slice(),
        std::slice::from_ref(&origin)
    );

    bridge.notify(Event::Locked);
    let event: EventFrame =
        tokio::time::timeout(Duration::from_secs(10), read_message(&mut stdout))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(
        event,
        EventFrame::Event {
            event: Event::Locked
        }
    );

    // The browser closes the port: the host exits.
    drop(stdin);
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(status.success());

    // An extension the manifests do not list is turned away.
    let stranger = spawn(
        "chrome-extension://ponmlkjihgfedcbaponmlkjihgfedcba/",
        &pipe,
    );
    let status = tokio::time::timeout(Duration::from_secs(10), stranger.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(!status.status.success());
    assert!(status.stdout.is_empty());

    // With no app running, the host says so once and exits.
    let alone = spawn(&origin, r"\\.\pipe\kandado-bridge-test-nobody-here");
    let output = tokio::time::timeout(Duration::from_secs(10), alone.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&output.stdout[4..], br#"{"type":"unavailable"}"#);

    install::uninstall(&host(), &NoRegistry).unwrap();
    drop(bridge);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_program_cannot_take_an_existing_pipe() {
    let pipe = format!(
        r"\\.\pipe\kandado-bridge-test-taken-{}",
        Uuid::from_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    );
    let handle = tokio::runtime::Handle::current();
    let first = Bridge::start_named(&handle, pipe.clone(), Arc::new(App::default())).unwrap();
    assert!(Bridge::start_named(&handle, pipe, Arc::new(App::default())).is_err());
    drop(first);
}
