//! The updater against a fake release server: it takes a newer, correctly
//! signed update and refuses everything else.

use std::{
    collections::HashMap,
    io::Cursor,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use kandado_app::update::{UpdateStatus, Updater};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpListener,
};

type Files = Arc<Mutex<HashMap<String, Vec<u8>>>>;

/// Serves the files put in the map, by path, over plain HTTP; anything else
/// is a 404. The address comes first, since the manifest names it.
async fn serve() -> (String, Files) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let files: Files = Arc::default();
    let served = files.clone();
    tokio::spawn(async move {
        let files = served;
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let files = files.clone();
            tokio::spawn(async move {
                let mut request = Vec::new();
                let mut buffer = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match socket.read(&mut buffer).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => request.extend_from_slice(&buffer[..n]),
                    }
                }
                let line = String::from_utf8_lossy(&request);
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let body = files.lock().unwrap().get(&path).cloned();
                let reply = match body {
                    Some(body) => {
                        let mut reply = format!(
                            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        reply.extend_from_slice(&body);
                        reply
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                let _ = socket.write_all(&reply).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    (format!("http://{address}"), files)
}

struct Release {
    keys: minisign::KeyPair,
    installer: Vec<u8>,
}

impl Release {
    fn new() -> Self {
        Self {
            keys: minisign::KeyPair::generate_unencrypted_keypair().unwrap(),
            installer: b"MZ pretend installer".repeat(1000),
        }
    }

    fn public_key(&self) -> String {
        self.keys.pk.to_base64()
    }

    fn manifest(&self, base: &str, version: &str, sha256: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "version": version,
            "notes_url": "https://example.test/notes",
            "windows": {
                "url": format!("{base}/kandado-{version}-setup.exe"),
                "sha256": sha256,
                "size": self.installer.len(),
            },
        }))
        .unwrap()
    }

    fn sha256(&self) -> String {
        Sha256::digest(&self.installer)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn sign(&self, data: &[u8], trusted: &str) -> Vec<u8> {
        minisign::sign(
            Some(&self.keys.pk),
            &self.keys.sk,
            Cursor::new(data),
            Some(trusted),
            Some("test"),
        )
        .unwrap()
        .into_string()
        .into_bytes()
    }

    /// A server for this release, with its files changed by `edit`
    /// (manifest, signature, installer).
    async fn server(
        &self,
        version: &str,
        edit: impl FnOnce(&mut Vec<u8>, &mut Vec<u8>, &mut Vec<u8>),
    ) -> String {
        let (base, files) = serve().await;
        let mut manifest = self.manifest(&base, version, &self.sha256());
        let mut signature = self.sign(&manifest, "kandado update latest.json");
        let mut installer = self.installer.clone();
        edit(&mut manifest, &mut signature, &mut installer);
        let mut files = files.lock().unwrap();
        files.insert("/latest.json".to_owned(), manifest);
        files.insert("/latest.json.minisig".to_owned(), signature);
        files.insert(format!("/kandado-{version}-setup.exe"), installer);
        base
    }
}

fn folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("kandado-update-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    folder
}

async fn check(
    release: &Release,
    base: &str,
    name: &str,
    installed: bool,
) -> (UpdateStatus, PathBuf) {
    let data = folder(name);
    let updater = Updater::with(base, &release.public_key(), "0.1.0", &data, installed);
    updater.check().await;
    (updater.status(), data)
}

#[tokio::test]
async fn a_newer_signed_update_is_downloaded_and_verified() {
    let release = Release::new();
    let base = release.server("0.2.0", |_, _, _| {}).await;
    let data = folder("good");
    let updater = Updater::with(&base, &release.public_key(), "0.1.0", &data, true);
    updater.check().await;
    assert_eq!(
        updater.status(),
        UpdateStatus::Ready {
            version: "0.2.0".into()
        }
    );
    let installer = updater.ready_installer().unwrap();
    assert_eq!(std::fs::read(&installer).unwrap(), release.installer);
    assert_eq!(installer.file_name().unwrap(), "kandado-0.2.0-setup.exe");
    let _ = std::fs::remove_dir_all(data);
}

#[tokio::test]
async fn the_same_or_an_older_version_is_up_to_date() {
    let release = Release::new();
    for (version, name) in [("0.1.0", "same"), ("0.0.9", "older")] {
        let base = release.server(version, |_, _, _| {}).await;
        let (status, data) = check(&release, &base, name, true).await;
        assert_eq!(status, UpdateStatus::UpToDate, "{version}");
        assert!(!data.join("updates").exists());
        let _ = std::fs::remove_dir_all(data);
    }
}

#[tokio::test]
async fn a_portable_copy_only_says_a_version_is_available() {
    let release = Release::new();
    let base = release.server("0.2.0", |_, _, _| {}).await;
    let (status, data) = check(&release, &base, "portable", false).await;
    assert_eq!(
        status,
        UpdateStatus::Available {
            version: "0.2.0".into(),
            notes_url: "https://example.test/notes".into()
        }
    );
    let _ = std::fs::remove_dir_all(data);
}

#[tokio::test]
async fn a_changed_manifest_fails_the_signature() {
    let release = Release::new();
    let base = release
        .server("0.2.0", |manifest, _, _| {
            *manifest = String::from_utf8(manifest.clone())
                .unwrap()
                .replace("0.2.0", "9.0.0")
                .into_bytes();
        })
        .await;
    let (status, data) = check(&release, &base, "tampered", true).await;
    assert_eq!(
        status,
        UpdateStatus::Failed("the update's signature is not valid".into())
    );
    let _ = std::fs::remove_dir_all(data);
}

#[tokio::test]
async fn another_key_or_another_signed_file_is_refused() {
    let release = Release::new();
    let other = Release::new();
    let base = release
        .server("0.2.0", |manifest, signature, _| {
            *signature = other.sign(manifest, "kandado update latest.json");
        })
        .await;
    let (status, data) = check(&release, &base, "other-key", true).await;
    assert_eq!(
        status,
        UpdateStatus::Failed("the update's signature is not valid".into())
    );
    let _ = std::fs::remove_dir_all(data);

    // Right key, but signed as some other file.
    let base = release
        .server("0.2.0", |manifest, signature, _| {
            *signature = release.sign(manifest, "kandado update beta.json");
        })
        .await;
    let (status, data) = check(&release, &base, "other-file", true).await;
    assert_eq!(
        status,
        UpdateStatus::Failed("the update's signature is not valid".into())
    );
    let _ = std::fs::remove_dir_all(data);
}

#[tokio::test]
async fn a_changed_installer_is_not_kept() {
    let release = Release::new();
    let base = release
        .server("0.2.0", |_, _, installer| installer[0] ^= 1)
        .await;
    let data = folder("bad-installer");
    let updater = Updater::with(&base, &release.public_key(), "0.1.0", &data, true);
    updater.check().await;
    assert_eq!(
        updater.status(),
        UpdateStatus::Failed("the download does not match the signed update".into())
    );
    assert!(updater.ready_installer().is_none());
    let leftovers: Vec<_> = std::fs::read_dir(data.join("updates")).unwrap().collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    assert!(updater.install().is_err());
    let _ = std::fs::remove_dir_all(data);
}

#[tokio::test]
async fn no_server_is_a_network_failure() {
    let release = Release::new();
    let (status, data) = check(&release, "http://127.0.0.1:9", "offline", true).await;
    assert_eq!(
        status,
        UpdateStatus::Failed("couldn't reach the update server".into())
    );
    let _ = std::fs::remove_dir_all(data);
}
