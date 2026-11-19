//! The browser extension link (D-45): pairing, and unlocking paired extensions
//! when this app is unlocked.
//!
//! "Unlock the browser extension with this app" registers the native messaging
//! host with Chrome, Edge and Firefox and starts the pipe server
//! (`kandado-bridge`). Turning it off undoes both.
//!
//! - **Pairing.** An extension sends its pairing public key. The app shows the
//!   six-digit code made from it (`kandado_crypto::pairing::code`); the person
//!   checks the extension shows the same code and accepts. Only while unlocked,
//!   one request at a time, two minutes to answer.
//! - **Unlock.** A paired extension asks; if the app is unlocked and the setting
//!   is on, it gets the user key sealed to its public key. Only that extension's
//!   private key opens it.
//! - **Events.** Every connected extension hears when the app locks or unlocks.
//!
//! Pairings are kept in `pairings.json`: ids, browser names and public keys,
//! nothing secret.

use std::{
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use kandado_bridge::{
    install::AllowedExtensions,
    protocol::{AppState, Event, Refusal, ResponseBody},
    server::{Bridge, Handler},
};
use serde::{Deserialize, Serialize};
use tokio::{runtime::Handle, sync::oneshot};
use uuid::Uuid;

use crate::{AppError, KandadoApp};

/// The development build of the extension (fixed by `manifests/dev.json`).
pub const DEV_CHROMIUM_ID: &str = "knfanepenpkmplikmbdpmgnmpjfaljco";
/// The Firefox add-on ID, from the extension's Firefox manifest.
pub const FIREFOX_ID: &str = "kandado@kandado.app";
/// How long a pairing request waits for the person.
pub const PAIRING_TIMEOUT: Duration = Duration::from_secs(120);

/// The extensions allowed to start the native host. Store IDs are added when
/// the extension is first published (decision P5-7); debug builds also trust
/// the development ID, and `KANDADO_EXTENSION_IDS` adds more (comma separated).
pub fn allowed_extensions() -> AllowedExtensions {
    let mut chromium_ids: Vec<String> = Vec::new();
    if cfg!(debug_assertions) {
        chromium_ids.push(DEV_CHROMIUM_ID.to_owned());
    }
    if let Ok(extra) = std::env::var("KANDADO_EXTENSION_IDS") {
        chromium_ids.extend(extra.split(',').map(|id| id.trim().to_owned()));
    }
    AllowedExtensions {
        chromium_ids,
        firefox_ids: vec![FIREFOX_ID.to_owned()],
    }
    .cleaned()
}

/// One paired extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pairing {
    pub id: Uuid,
    /// The account it was paired under. It unlocks only for that account.
    pub user_id: Uuid,
    pub browser: String,
    pub origin: String,
    /// Its X25519 public key.
    pub public_key: [u8; 32],
    /// Unix seconds.
    pub paired_at: i64,
}

/// A pairing waiting for the person, as the app shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingRequest {
    pub id: Uuid,
    pub browser: String,
    /// The six digits the extension shows too.
    pub code: String,
}

struct Pending {
    request: PairingRequest,
    pairing: Pairing,
    answer: oneshot::Sender<bool>,
}

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("the browser link program (kandado-native-host) is missing from Kandado's folder")]
    HostMissing,
    #[error("the browsers could not be told about Kandado: {0}")]
    Install(String),
    #[error("the browser link could not start: {0}")]
    Pipe(String),
    #[error("the paired browsers could not be saved: {0}")]
    Save(String),
    #[error(transparent)]
    App(#[from] AppError),
}

pub struct BrowserLink {
    app: Arc<KandadoApp>,
    /// Runs the pipe server, which exists only on Windows.
    #[cfg_attr(not(windows), allow(dead_code))]
    runtime: Handle,
    path: PathBuf,
    pairings: Mutex<Vec<Pairing>>,
    pending: Mutex<Option<Pending>>,
    bridge: Mutex<Option<Bridge>>,
    /// Bumped on every change, so views know to draw again.
    version: Mutex<u64>,
}

fn guard<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().expect("a panic poisoned the browser link")
}

impl BrowserLink {
    pub fn new(app: Arc<KandadoApp>, runtime: Handle, data_dir: PathBuf) -> Arc<Self> {
        let path = data_dir.join("pairings.json");
        let pairings = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Arc::new(Self {
            app,
            runtime,
            path,
            pairings: Mutex::new(pairings),
            pending: Mutex::new(None),
            bridge: Mutex::new(None),
            version: Mutex::new(0),
        })
    }

    /// `kandado-native-host.exe`, next to the app.
    pub fn host_exe() -> PathBuf {
        let name = if cfg!(windows) {
            "kandado-native-host.exe"
        } else {
            "kandado-native-host"
        };
        let beside = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
            .unwrap_or_else(|| PathBuf::from(name));
        // Development: Cargo puts examples and tests one folder below the host.
        if cfg!(debug_assertions)
            && !beside.exists()
            && let Some(above) = beside.parent().and_then(|dir| dir.parent())
        {
            let candidate = above.join(name);
            if candidate.exists() {
                return candidate;
            }
        }
        beside
    }

    pub fn version(&self) -> u64 {
        *guard(&self.version)
    }

    fn bump(&self) {
        *guard(&self.version) += 1;
    }

    pub fn enabled(&self) -> bool {
        self.app.settings().unlock_extension
    }

    /// Whether the pipe server is running.
    pub fn running(&self) -> bool {
        guard(&self.bridge).is_some()
    }

    /// At startup: starts the link if the setting is on.
    pub fn start(self: &Arc<Self>) -> Result<(), LinkError> {
        if self.enabled() {
            self.switch_on()?;
        }
        Ok(())
    }

    /// The D-45 toggle.
    pub fn set_enabled(self: &Arc<Self>, on: bool) -> Result<(), LinkError> {
        if on {
            self.switch_on()?;
        } else {
            self.switch_off()?;
        }
        self.app
            .update_settings(|settings| settings.unlock_extension = on)?;
        self.bump();
        Ok(())
    }

    #[cfg(windows)]
    fn switch_on(self: &Arc<Self>) -> Result<(), LinkError> {
        let host = Self::host_exe();
        if !host.exists() {
            return Err(LinkError::HostMissing);
        }
        kandado_bridge::install::install(
            &host,
            &allowed_extensions(),
            &kandado_bridge::install::WindowsRegistry,
        )
        .map_err(|error| LinkError::Install(error.to_string()))?;
        let mut bridge = guard(&self.bridge);
        if bridge.is_none() {
            let handler = Arc::new(LinkHandler(Arc::downgrade(self)));
            *bridge = Some(
                Bridge::start(&self.runtime, handler)
                    .map_err(|error| LinkError::Pipe(error.to_string()))?,
            );
            self.watch_lock_state();
        }
        Ok(())
    }

    #[cfg(not(windows))]
    fn switch_on(self: &Arc<Self>) -> Result<(), LinkError> {
        Err(LinkError::Pipe("only built for Windows".into()))
    }

    fn switch_off(&self) -> Result<(), LinkError> {
        guard(&self.bridge).take();
        if let Some(pending) = guard(&self.pending).take() {
            let _ = pending.answer.send(false);
        }
        #[cfg(windows)]
        kandado_bridge::install::uninstall(
            &Self::host_exe(),
            &kandado_bridge::install::WindowsRegistry,
        )
        .map_err(|error| LinkError::Install(error.to_string()))?;
        Ok(())
    }

    /// Tells connected extensions when the app locks or unlocks.
    #[cfg(windows)]
    fn watch_lock_state(self: &Arc<Self>) {
        let link = Arc::downgrade(self);
        self.runtime.spawn(async move {
            let mut last: Option<bool> = None;
            loop {
                let Some(link) = link.upgrade() else { return };
                if !link.running() {
                    return;
                }
                let unlocked = link.app.is_unlocked();
                if last.is_some_and(|before| before != unlocked)
                    && let Some(bridge) = guard(&link.bridge).as_ref()
                {
                    bridge.notify(if unlocked {
                        Event::Unlocked
                    } else {
                        Event::Locked
                    });
                }
                last = Some(unlocked);
                drop(link);
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
        });
    }

    // ---- pairings -----------------------------------------------------------------

    /// The pairings of the signed-in account.
    pub fn pairings(&self) -> Vec<Pairing> {
        let Some(user) = self.app.user_id() else {
            return Vec::new();
        };
        guard(&self.pairings)
            .iter()
            .filter(|pairing| pairing.user_id == user)
            .cloned()
            .collect()
    }

    pub fn remove(&self, id: Uuid) -> Result<(), LinkError> {
        guard(&self.pairings).retain(|pairing| pairing.id != id);
        self.save()?;
        self.bump();
        Ok(())
    }

    fn save(&self) -> Result<(), LinkError> {
        let bytes = serde_json::to_vec_pretty(&*guard(&self.pairings))
            .map_err(|error| LinkError::Save(error.to_string()))?;
        std::fs::write(&self.path, bytes).map_err(|error| LinkError::Save(error.to_string()))
    }

    /// A pairing waiting for the person, if any.
    pub fn pending(&self) -> Option<PairingRequest> {
        guard(&self.pending)
            .as_ref()
            .map(|pending| pending.request.clone())
    }

    /// The person accepted or refused the pairing with this id.
    pub fn answer(&self, id: Uuid, accept: bool) -> Result<(), LinkError> {
        let Some(pending) = guard(&self.pending).take() else {
            return Ok(());
        };
        if pending.request.id != id {
            *guard(&self.pending) = Some(pending);
            return Ok(());
        }
        if accept {
            let mut pairings = guard(&self.pairings);
            pairings.retain(|pairing| pairing.id != pending.pairing.id);
            pairings.push(pending.pairing);
            drop(pairings);
            self.save()?;
        }
        let _ = pending.answer.send(accept);
        self.bump();
        Ok(())
    }

    // ---- answers to the bridge ---------------------------------------------------------

    fn status(&self) -> ResponseBody {
        let user_id = self.app.user_id();
        let state = match (user_id, self.app.is_unlocked()) {
            (None, _) => AppState::SignedOut,
            (Some(_), false) => AppState::Locked,
            (Some(_), true) => AppState::Unlocked,
        };
        ResponseBody::Status {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            state,
            user_id,
            unlock_enabled: self.enabled(),
        }
    }

    fn request_pairing(
        self: &Arc<Self>,
        origin: String,
        pairing_id: Uuid,
        public_key: [u8; 32],
        browser: String,
    ) -> Pin<Box<dyn Future<Output = ResponseBody> + Send>> {
        let refused = |reason| -> Pin<Box<dyn Future<Output = ResponseBody> + Send>> {
            Box::pin(async move { ResponseBody::Refused { reason } })
        };
        if !self.enabled() {
            return refused(Refusal::TurnedOff);
        }
        let Some(user_id) = self.app.user_id().filter(|_| self.app.is_unlocked()) else {
            return refused(Refusal::Locked);
        };
        let (answer, answered) = oneshot::channel();
        {
            let mut pending = guard(&self.pending);
            // One at a time: a second request while one waits is refused.
            if pending.is_some() {
                return refused(Refusal::Declined);
            }
            *pending = Some(Pending {
                request: PairingRequest {
                    id: pairing_id,
                    browser: browser.clone(),
                    code: kandado_crypto::pairing::code(&public_key),
                },
                pairing: Pairing {
                    id: pairing_id,
                    user_id,
                    browser,
                    origin,
                    public_key,
                    paired_at: time::OffsetDateTime::now_utc().unix_timestamp(),
                },
                answer,
            });
        }
        self.bump();
        let link = Arc::downgrade(self);
        Box::pin(async move {
            let accepted = tokio::time::timeout(PAIRING_TIMEOUT, answered).await;
            match accepted {
                Ok(Ok(true)) => ResponseBody::Paired,
                Ok(_) => ResponseBody::Refused {
                    reason: Refusal::Declined,
                },
                Err(_) => {
                    // Timed out: the request leaves the screen.
                    if let Some(link) = link.upgrade() {
                        let mut pending = guard(&link.pending);
                        if pending
                            .as_ref()
                            .is_some_and(|waiting| waiting.request.id == pairing_id)
                        {
                            pending.take();
                        }
                        drop(pending);
                        link.bump();
                    }
                    ResponseBody::Refused {
                        reason: Refusal::Declined,
                    }
                }
            }
        })
    }

    fn unlock(&self, pairing_id: Uuid) -> ResponseBody {
        let refused = |reason| ResponseBody::Refused { reason };
        if !self.enabled() {
            return refused(Refusal::TurnedOff);
        }
        let Some(user) = self.app.user_id() else {
            return refused(Refusal::Locked);
        };
        let Some(pairing) = guard(&self.pairings)
            .iter()
            .find(|pairing| pairing.id == pairing_id && pairing.user_id == user)
            .cloned()
        else {
            return refused(Refusal::NotPaired);
        };
        match self
            .app
            .seal_user_key_for_extension(pairing.id, &pairing.public_key)
        {
            Ok(Some(sealed)) => {
                use base64::{Engine, engine::general_purpose::STANDARD};
                ResponseBody::Unlocked {
                    sealed: STANDARD.encode(sealed),
                }
            }
            Ok(None) => refused(Refusal::Locked),
            Err(error) => {
                tracing::warn!(%error, "the user key could not be sealed for an extension");
                refused(Refusal::Locked)
            }
        }
    }

    fn unpair(&self, pairing_id: Uuid) -> ResponseBody {
        let _ = self.remove(pairing_id);
        ResponseBody::Done
    }
}

/// The bridge's view of the link. Weak, so the pipe server does not keep the app alive.
struct LinkHandler(Weak<BrowserLink>);

impl Handler for LinkHandler {
    fn status(&self) -> ResponseBody {
        match self.0.upgrade() {
            Some(link) => link.status(),
            None => ResponseBody::Refused {
                reason: Refusal::Locked,
            },
        }
    }

    fn pair(
        &self,
        origin: String,
        pairing_id: Uuid,
        public_key: [u8; 32],
        browser: String,
    ) -> Pin<Box<dyn Future<Output = ResponseBody> + Send>> {
        match self.0.upgrade() {
            Some(link) => link.request_pairing(origin, pairing_id, public_key, browser),
            None => Box::pin(async {
                ResponseBody::Refused {
                    reason: Refusal::Locked,
                }
            }),
        }
    }

    fn unlock(&self, pairing_id: Uuid) -> ResponseBody {
        match self.0.upgrade() {
            Some(link) => link.unlock(pairing_id),
            None => ResponseBody::Refused {
                reason: Refusal::Locked,
            },
        }
    }

    fn unpair(&self, pairing_id: Uuid) -> ResponseBody {
        match self.0.upgrade() {
            Some(link) => link.unpair(pairing_id),
            None => ResponseBody::Done,
        }
    }
}

/// For tests: the same answers the pipe server gets, without the pipe.
#[doc(hidden)]
pub fn handler_for_tests(link: &Arc<BrowserLink>) -> Arc<dyn Handler> {
    Arc::new(LinkHandler(Arc::downgrade(link)))
}

/// For tests: turns the setting on without registering anything with the browsers.
#[doc(hidden)]
pub fn enable_for_tests(link: &BrowserLink) -> Result<(), LinkError> {
    link.app
        .update_settings(|settings| settings.unlock_extension = true)?;
    Ok(())
}
