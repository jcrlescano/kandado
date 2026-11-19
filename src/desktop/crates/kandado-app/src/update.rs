//! Updates (D-54): find a newer Kandado, download it, and install it when the
//! person chooses Restart.
//!
//! Each GitHub Release carries `latest.json` and `latest.json.minisig`
//! (made by `src/desktop/tools/update-signer` in `release.yml`):
//!
//! ```json
//! { "version": "0.2.0", "notes_url": "https://…",
//!   "windows": { "url": "https://…/kandado-0.2.0-setup.exe", "sha256": "…", "size": 123 } }
//! ```
//!
//! The manifest is trusted only if its Minisign signature checks out against
//! the public key compiled into the app (`update_key.pub`) and its trusted
//! comment names `latest.json`. The installer is trusted only if its size and
//! SHA-256 match the signed manifest. So neither GitHub nor anyone who can
//! change a release can push an update without the signing key. Only a
//! version newer than this one is taken; nothing ever goes backwards.
//!
//! The app checks about a minute after it starts and then every 12 hours,
//! while "Check for updates automatically" is on, and whenever the person
//! presses Check now. A ready installer runs with `/S /RELAUNCH` (silent, then
//! start Kandado again) after the app locks and quits.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::{io::AsyncWriteExt as _, sync::Notify};

/// The key that signs `latest.json`. Its secret half is the
/// `UPDATE_SIGNING_KEY` secret of the release workflow.
pub const PUBLIC_KEY: &str = include_str!("update_key.pub");

/// Where the latest release's files are. `KANDADO_UPDATE_URL` overrides it.
pub const DEFAULT_UPDATE_URL: &str =
    "https://github.com/jcrlescano/kandado/releases/latest/download";

/// This app's version.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

const FIRST_CHECK: Duration = Duration::from_secs(60);
const CHECK_EVERY: Duration = Duration::from_secs(12 * 60 * 60);
const MANIFEST_LIMIT: usize = 64 * 1024;
const INSTALLER_LIMIT: u64 = 300 * 1024 * 1024;
const SIGNED_NAME: &str = "kandado update latest.json";

/// What the updater is doing, for the banner and the Updates section.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UpdateStatus {
    /// Nothing checked yet.
    #[default]
    Idle,
    Checking,
    /// This is the newest version.
    UpToDate,
    Downloading {
        version: String,
    },
    /// Downloaded and verified: Restart installs it.
    Ready {
        version: String,
    },
    /// A newer version exists but this copy is not installed (the zip), so
    /// it cannot update itself: the person downloads it.
    Available {
        version: String,
        notes_url: String,
    },
    /// The last check failed; it is tried again later.
    Failed(String),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UpdateError {
    #[error("couldn't reach the update server")]
    Network,
    #[error("the update server answered {0}")]
    Status(u16),
    #[error("the update's signature is not valid")]
    BadSignature,
    #[error("the update's description is not valid")]
    BadManifest,
    #[error("the download does not match the signed update")]
    Mismatch,
    #[error("the update could not be saved: {0}")]
    Io(String),
    #[error("no update is ready")]
    NotReady,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub notes_url: String,
    pub windows: Download,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Download {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// Checks the signature and the version. `Ok(None)` when the manifest is
/// genuine but not newer than `current`.
pub fn verify_manifest(
    public_key: &str,
    manifest: &[u8],
    signature: &str,
    current: &str,
) -> Result<Option<Manifest>, UpdateError> {
    let key = minisign_verify::PublicKey::from_base64(public_key.trim())
        .map_err(|_| UpdateError::BadSignature)?;
    let signature =
        minisign_verify::Signature::decode(signature).map_err(|_| UpdateError::BadSignature)?;
    key.verify(manifest, &signature, false)
        .map_err(|_| UpdateError::BadSignature)?;
    // The trusted comment is covered by the signature: it stops another file
    // signed with the same key from being passed off as latest.json.
    if signature.trusted_comment() != SIGNED_NAME {
        return Err(UpdateError::BadSignature);
    }
    let manifest: Manifest =
        serde_json::from_slice(manifest).map_err(|_| UpdateError::BadManifest)?;
    let (Some(new), Some(old)) = (parse_version(&manifest.version), parse_version(current)) else {
        return Err(UpdateError::BadManifest);
    };
    if manifest.windows.sha256.len() != 64
        || !manifest
            .windows
            .sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
        || manifest.windows.size == 0
        || manifest.windows.size > INSTALLER_LIMIT
    {
        return Err(UpdateError::BadManifest);
    }
    Ok((new > old).then_some(manifest))
}

/// `1.2.3` as numbers; anything else is `None`.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.split('.');
    let mut next = || -> Option<u64> {
        let part = parts.next()?;
        (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse().ok())
            .flatten()
    };
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(version)
}

/// Finds, downloads and installs updates. One per app.
pub struct Updater {
    base: String,
    public_key: String,
    current: String,
    folder: PathBuf,
    http: reqwest::Client,
    /// This copy was installed by the installer, so it can update itself.
    installed: bool,
    status: Mutex<UpdateStatus>,
    ready: Mutex<Option<PathBuf>>,
    check_now: Notify,
}

impl Updater {
    /// `data_dir` holds the `updates` folder for downloads.
    pub fn new(data_dir: &Path) -> Arc<Self> {
        let base = std::env::var("KANDADO_UPDATE_URL")
            .ok()
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| DEFAULT_UPDATE_URL.to_owned());
        Self::with(&base, PUBLIC_KEY, CURRENT_VERSION, data_dir, is_installed())
    }

    /// For tests: another server, key and version.
    pub fn with(
        base: &str,
        public_key: &str,
        current: &str,
        data_dir: &Path,
        installed: bool,
    ) -> Arc<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(20))
            // GitHub answers release downloads with a redirect to its storage.
            .redirect(reqwest::redirect::Policy::limited(5))
            .https_only(base.starts_with("https://"))
            .user_agent(concat!("Kandado-Desktop/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("the update client builds");
        Arc::new(Self {
            base: base.trim_end_matches('/').to_owned(),
            public_key: public_key.to_owned(),
            current: current.to_owned(),
            folder: data_dir.join("updates"),
            http,
            installed,
            status: Mutex::new(UpdateStatus::Idle),
            ready: Mutex::new(None),
            check_now: Notify::new(),
        })
    }

    pub fn current_version(&self) -> &str {
        &self.current
    }

    pub fn status(&self) -> UpdateStatus {
        lock(&self.status).clone()
    }

    fn set_status(&self, status: UpdateStatus) {
        *lock(&self.status) = status;
    }

    /// Checks in the background for as long as the app runs: about a minute
    /// after start, then every 12 hours while `enabled()` says so, and
    /// whenever [`Updater::check_soon`] asks.
    pub fn start(
        self: &Arc<Self>,
        runtime: &tokio::runtime::Handle,
        enabled: impl Fn() -> bool + Send + 'static,
    ) {
        let this = self.clone();
        runtime.spawn(async move {
            let mut wait = FIRST_CHECK;
            loop {
                let asked = tokio::select! {
                    () = tokio::time::sleep(wait) => false,
                    () = this.check_now.notified() => true,
                };
                wait = CHECK_EVERY;
                if asked || enabled() {
                    this.check().await;
                }
            }
        });
    }

    /// Check now (the Check now button).
    pub fn check_soon(&self) {
        self.check_now.notify_one();
    }

    /// One check: fetch and verify the manifest, then download the installer
    /// if there is a newer version. The outcome is in [`Updater::status`].
    pub async fn check(&self) {
        if matches!(self.status(), UpdateStatus::Ready { .. }) {
            return;
        }
        self.set_status(UpdateStatus::Checking);
        let status = match self.find().await {
            Ok(None) => UpdateStatus::UpToDate,
            Ok(Some(manifest)) if !self.installed => UpdateStatus::Available {
                version: manifest.version,
                notes_url: manifest.notes_url,
            },
            Ok(Some(manifest)) => {
                self.set_status(UpdateStatus::Downloading {
                    version: manifest.version.clone(),
                });
                match self.download(&manifest).await {
                    Ok(path) => {
                        *lock(&self.ready) = Some(path);
                        UpdateStatus::Ready {
                            version: manifest.version,
                        }
                    }
                    Err(error) => failed(&error),
                }
            }
            Err(error) => failed(&error),
        };
        self.set_status(status);
    }

    async fn find(&self) -> Result<Option<Manifest>, UpdateError> {
        let manifest = self.fetch(&format!("{}/latest.json", self.base)).await?;
        let signature = self
            .fetch(&format!("{}/latest.json.minisig", self.base))
            .await?;
        let signature = String::from_utf8(signature).map_err(|_| UpdateError::BadSignature)?;
        verify_manifest(&self.public_key, &manifest, &signature, &self.current)
    }

    async fn fetch(&self, url: &str) -> Result<Vec<u8>, UpdateError> {
        let mut response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|_| UpdateError::Network)?;
        if !response.status().is_success() {
            return Err(UpdateError::Status(response.status().as_u16()));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| UpdateError::Network)? {
            body.extend_from_slice(&chunk);
            if body.len() > MANIFEST_LIMIT {
                return Err(UpdateError::BadManifest);
            }
        }
        Ok(body)
    }

    /// Downloads the installer to `updates/`, checking size and SHA-256
    /// against the signed manifest, and removes older downloads.
    async fn download(&self, manifest: &Manifest) -> Result<PathBuf, UpdateError> {
        let io = |error: std::io::Error| UpdateError::Io(error.to_string());
        let wanted = &manifest.windows;
        if self.base.starts_with("https://") && !wanted.url.starts_with("https://") {
            return Err(UpdateError::BadManifest);
        }
        tokio::fs::create_dir_all(&self.folder).await.map_err(io)?;
        let name = format!("kandado-{}-setup.exe", manifest.version);
        let path = self.folder.join(&name);
        let partial = self.folder.join(format!("{name}.part"));

        let mut response = self
            .http
            .get(&wanted.url)
            .send()
            .await
            .map_err(|_| UpdateError::Network)?;
        if !response.status().is_success() {
            return Err(UpdateError::Status(response.status().as_u16()));
        }
        let mut file = tokio::fs::File::create(&partial).await.map_err(io)?;
        let mut hash = Sha256::new();
        let mut size = 0u64;
        let result = async {
            while let Some(chunk) = response.chunk().await.map_err(|_| UpdateError::Network)? {
                size += chunk.len() as u64;
                if size > wanted.size {
                    return Err(UpdateError::Mismatch);
                }
                hash.update(&chunk);
                file.write_all(&chunk).await.map_err(io)?;
            }
            file.flush().await.map_err(io)?;
            Ok(())
        }
        .await;
        drop(file);
        let digest: String = hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let result = result.and_then(|()| {
            if size == wanted.size && digest.eq_ignore_ascii_case(&wanted.sha256) {
                Ok(())
            } else {
                Err(UpdateError::Mismatch)
            }
        });
        if let Err(error) = result {
            let _ = tokio::fs::remove_file(&partial).await;
            return Err(error);
        }
        tokio::fs::rename(&partial, &path).await.map_err(io)?;
        self.remove_old_downloads(&name).await;
        Ok(path)
    }

    async fn remove_old_downloads(&self, keep: &str) {
        let Ok(mut entries) = tokio::fs::read_dir(&self.folder).await else {
            return;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            if entry.file_name().to_str() != Some(keep) {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }
    }

    /// Where the verified installer is, once one is ready.
    pub fn ready_installer(&self) -> Option<PathBuf> {
        lock(&self.ready).clone()
    }

    /// Starts the ready installer, silent, to start Kandado again when done.
    /// The caller locks the vault and quits right after: the installer waits
    /// for the app to close before it replaces it.
    pub fn install(&self) -> Result<(), UpdateError> {
        let path = self.ready_installer().ok_or(UpdateError::NotReady)?;
        // Checked again just before it runs: the file sat on disk meanwhile.
        if !path.is_file() {
            self.set_status(UpdateStatus::Idle);
            *lock(&self.ready) = None;
            return Err(UpdateError::NotReady);
        }
        std::process::Command::new(&path)
            .args(["/S", "/RELAUNCH"])
            .spawn()
            .map(|_| ())
            .map_err(|error| UpdateError::Io(error.to_string()))
    }
}

fn failed(error: &UpdateError) -> UpdateStatus {
    tracing::warn!(%error, "update check failed");
    UpdateStatus::Failed(error.to_string())
}

/// The installer puts `uninstall.exe` beside `kandado.exe`; the zip does not.
fn is_installed() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("uninstall.exe").is_file()))
        .unwrap_or(false)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.10.0"), Some((0, 10, 0)));
        assert!(parse_version("0.10.0") > parse_version("0.9.9"));
        for bad in [
            "1.2",
            "1.2.3.4",
            "v1.2.3",
            "1.2.x",
            "1..3",
            "",
            "1.2.3-beta",
        ] {
            assert_eq!(parse_version(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_embedded_key_is_a_minisign_key() {
        assert!(minisign_verify::PublicKey::from_base64(PUBLIC_KEY.trim()).is_ok());
    }
}
