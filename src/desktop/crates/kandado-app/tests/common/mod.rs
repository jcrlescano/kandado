//! What the end-to-end tests share: the real `kandado-server` running against
//! PostgreSQL, and computers that sign in to it the way the app does, with
//! the browser step played by the server's development route instead of Google.
#![allow(dead_code)]

use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Arc,
    time::Duration,
};

use kandado_app::{KandadoApp, Paths, Phase, device_key::MemoryDeviceKey, secrets::MemorySecrets};
use kandado_crypto::kdf::KdfParams;

pub struct Server {
    child: Child,
    pub url: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

pub async fn start_server() -> Server {
    start_server_on(free_port()).await
}

/// Starts the server on a chosen port, such as the one a stopped server had.
pub async fn start_server_on(port: u16) -> Server {
    // The workspace root: the nearest folder up with the lock file, so this
    // works from any crate that includes it.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|dir| dir.join("Cargo.lock").exists())
        .expect("inside the workspace")
        .to_path_buf();
    let exe = if cfg!(windows) {
        "kandado-server.exe"
    } else {
        "kandado-server"
    };
    let binary = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("target"))
        .join("debug")
        .join(exe);
    assert!(
        binary.exists(),
        "build the server first: cargo build -p kandado-server"
    );

    let child = Command::new(binary)
        .env("KANDADO_BIND", format!("127.0.0.1:{port}"))
        .env("KANDADO_PUBLIC_URL", format!("http://127.0.0.1:{port}"))
        .env(
            "KANDADO_TOKEN_SECRET",
            "e2e-secret-e2e-secret-e2e-secret-00",
        )
        .env("KANDADO_DEV_AUTH", "1")
        .env("KANDADO_VIEWER_DIR", root.join("src/backend/viewer/dist"))
        .env("KANDADO_LOG_FORMAT", "text")
        .env(
            "DATABASE_URL",
            std::env::var("DATABASE_URL").expect("set DATABASE_URL"),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let url = format!("http://127.0.0.1:{port}");
    let mut server = Server { child, url };
    let client = reqwest::Client::new();
    for _ in 0..100 {
        if let Ok(response) = client.get(format!("{}/readyz", server.url)).send().await
            && response.status().is_success()
        {
            return server;
        }
        if server.child.try_wait().unwrap().is_some() {
            panic!("the server exited early; is PostgreSQL running?");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the server did not become ready");
}

/// One computer: its own data folder, secret store and device key.
pub fn computer(name: &str, server: &Server) -> Arc<KandadoApp> {
    let dir = std::env::temp_dir().join(format!(
        "kandado-e2e-{}-{name}-{}",
        std::process::id(),
        server.url.rsplit(':').next().unwrap()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    KandadoApp::with_params(
        Paths { data_dir: dir },
        Arc::new(MemorySecrets::default()),
        Arc::new(MemoryDeviceKey::new()),
        KdfParams::MIN,
        Some(&server.url),
    )
    .unwrap()
}

/// Signs in the way the app does, with the browser step played by the
/// development route instead of Google.
pub async fn google(app: &Arc<KandadoApp>, server: &Server, email: &str) -> Option<Phase> {
    try_google(app, server, email).await.unwrap()
}

pub async fn try_google(
    app: &Arc<KandadoApp>,
    server: &Server,
    email: &str,
) -> Result<Option<Phase>, kandado_app::AppError> {
    let pending = app.begin_google().await.unwrap();
    let mut url = pending.url.clone();
    url.set_path("/v1/dev/sign-in");
    url.query_pairs_mut().append_pair("email", email);
    let worker = app.clone();
    let waiting = tokio::spawn(async move { worker.finish_google(pending).await });
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let reply = client.get(url).send().await.unwrap();
    assert_eq!(reply.status(), 303, "{server:?}", server = server.url);
    let location = reply.headers()["location"].to_str().unwrap().to_owned();
    // The browser follows the redirect to the app's loopback listener.
    client.get(location).send().await.unwrap();
    waiting.await.unwrap()
}

pub fn unique_email() -> String {
    let mut bytes = [0u8; 6];
    getrandom_fill(&mut bytes);
    format!("e2e-{}@example.test", hex(&bytes))
}

fn getrandom_fill(bytes: &mut [u8]) {
    kandado_crypto::random::fill(bytes).unwrap();
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl Server {
    /// Stops the server, as if the network or the machine went away.
    pub fn stop(mut self) -> u16 {
        let port = self.url.rsplit(':').next().unwrap().parse().unwrap();
        let _ = self.child.kill();
        let _ = self.child.wait();
        // `Drop` runs on the emptied value: killing twice is harmless.
        port
    }
}
