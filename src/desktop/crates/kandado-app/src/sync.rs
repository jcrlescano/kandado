//! When to sync, and what the window may show about it.
//!
//! A sync runs when the vault unlocks, a little after anything changes here,
//! when the server says something changed elsewhere (the events connection),
//! on a timer as a safety net, and when the person asks. While the server
//! cannot be reached, tries are spaced further and further apart, up to five
//! minutes, and the window shows the offline banner (D-54); the first one that
//! works clears it. Nothing here blocks the person: every change is saved
//! locally first.

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use kandado_vault::{ApiError, Session, Store, VaultError, sync as engine, sync::Access};
use tokio::sync::Notify;

use crate::{service::KandadoApp, tasks::Tasks, vault::now};

/// A change made here waits this long for more, then goes out together.
const SETTLE: Duration = Duration::from_millis(400);
/// A safety net for a missed event: sync at least this often while unlocked.
const POLL: Duration = Duration::from_secs(5 * 60);
/// The first wait after a failed try, doubling up to [`BACKOFF_MAX`].
const BACKOFF_START: Duration = Duration::from_secs(5);
const BACKOFF_MAX: Duration = Duration::from_secs(5 * 60);
/// How often the scheduler checks whether the vault is unlocked.
const WATCH: Duration = Duration::from_millis(500);
/// How long the events connection waits before it is opened again.
const RECONNECT: Duration = Duration::from_secs(5);

/// Where syncing stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncState {
    /// Nothing is happening; `last_synced` says when it last worked.
    Idle,
    Syncing,
    /// The server could not be reached. Changes are safe here and go later.
    Offline,
    /// The server answered with an error. Retrying.
    Failed(SyncFailure),
}

/// Why a sync failed, in the few ways the person can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncFailure {
    /// The session ended on another device. Signing in again is needed.
    SignedOut,
    /// The server or the local file had a problem. Retrying.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncStatus {
    pub state: SyncState,
    /// Unix seconds of the last sync that worked, if any since the app started.
    pub last_synced: Option<i64>,
}

pub(crate) struct SyncControl {
    status: std::sync::Mutex<SyncStatus>,
    /// Set when a sync changed what the lists show.
    changes: AtomicBool,
    /// Wakes the scheduler: something to send, something to pull, or a request.
    wake: Notify,
    /// Skip the settle delay and the backoff once: the person asked.
    now: AtomicBool,
}

impl SyncControl {
    pub(crate) fn new() -> Self {
        Self {
            status: std::sync::Mutex::new(SyncStatus {
                state: SyncState::Idle,
                last_synced: None,
            }),
            changes: AtomicBool::new(false),
            wake: Notify::new(),
            now: AtomicBool::new(false),
        }
    }

    pub(crate) fn status(&self) -> SyncStatus {
        self.status.lock().expect("sync status").clone()
    }

    fn set(&self, state: SyncState, synced: bool) {
        let mut status = self.status.lock().expect("sync status");
        status.state = state;
        if synced {
            status.last_synced = Some(now());
        }
    }

    /// Something changed here, or on the server: sync soon.
    pub(crate) fn nudge(&self) {
        self.wake.notify_one();
    }

    pub(crate) fn reset(&self) {
        let mut status = self.status.lock().expect("sync status");
        *status = SyncStatus {
            state: SyncState::Idle,
            last_synced: None,
        };
        self.changes.store(false, Ordering::SeqCst);
    }
}

/// The app's store and session, for the sync engine.
struct Locked<'a>(&'a KandadoApp);

impl Access for Locked<'_> {
    fn with<T>(
        &self,
        work: impl FnOnce(&Store, &mut Session) -> Result<T, VaultError>,
    ) -> Result<T, VaultError> {
        self.0.raw_vault(work)
    }
}

fn failure_of(error: &engine::SyncError) -> SyncState {
    match error {
        engine::SyncError::Api(ApiError::Offline(_)) => SyncState::Offline,
        engine::SyncError::Api(ApiError::SignedOut) => SyncState::Failed(SyncFailure::SignedOut),
        _ => SyncState::Failed(SyncFailure::Other),
    }
}

impl KandadoApp {
    /// Where syncing stands, for the banner and the sidebar.
    pub fn sync_status(&self) -> SyncStatus {
        self.sync.status()
    }

    /// True once after a sync changed what the lists show, so the window
    /// draws them again.
    pub fn take_sync_changes(&self) -> bool {
        self.sync.changes.swap(false, Ordering::SeqCst)
    }

    /// Syncs soon, without waiting out a backoff: "Retry" and "Sync now".
    pub fn sync_soon(&self) {
        self.sync.now.store(true, Ordering::SeqCst);
        self.sync.nudge();
    }

    /// One sync, now. Does nothing while locked or signed out.
    pub async fn sync_now(&self) -> Result<engine::Report, engine::SyncError> {
        if !self.is_unlocked() {
            return Ok(engine::Report::default());
        }
        self.sync.set(SyncState::Syncing, false);
        let result = engine::run(self.api(), &Locked(self), now).await;
        match &result {
            Ok(report) => {
                if report.changed {
                    self.sync.changes.store(true, Ordering::SeqCst);
                }
                self.sync.set(SyncState::Idle, true);
            }
            Err(error) => {
                tracing::warn!(%error, "sync did not finish");
                self.sync.set(failure_of(error), false);
            }
        }
        result
    }

    /// Brings the versions the server has of an item into its history. Quietly
    /// does nothing offline: the history then shows what this computer has.
    pub async fn fetch_history(&self, id: uuid::Uuid) -> Result<usize, crate::AppError> {
        let versions = match self.api().item_versions(id).await {
            Ok(versions) => versions,
            Err(ApiError::Offline(_) | ApiError::Problem(_)) => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        self.vault(|store, session| {
            kandado_vault::history::merge_remote(store, session, id, &versions.versions)
        })
    }

    /// Starts the scheduler and the events connection. Call once, after the
    /// app is built. They run until the process ends.
    pub fn start_sync(self: &std::sync::Arc<Self>, tasks: &Tasks) {
        let app = self.clone();
        tasks.spawn_detached(async move { app.scheduler().await });
        let app = self.clone();
        tasks.spawn_detached(async move { app.listen().await });
    }

    fn signed_in_and_unlocked(&self) -> bool {
        self.is_unlocked()
    }

    async fn scheduler(self: std::sync::Arc<Self>) {
        let mut backoff = BACKOFF_START;
        loop {
            // Wait until the vault is open.
            while !self.signed_in_and_unlocked() {
                tokio::time::sleep(WATCH).await;
            }
            // The first sync after an unlock, then whenever woken or on time.
            let result = self.sync_now().await;
            let wait = match &result {
                Ok(_) => {
                    backoff = BACKOFF_START;
                    POLL
                }
                Err(error)
                    if matches!(failure_of(error), SyncState::Failed(SyncFailure::SignedOut)) =>
                {
                    // Nothing to retry until the person signs in again.
                    POLL
                }
                Err(_) => {
                    let wait = backoff;
                    backoff = (backoff * 2).min(BACKOFF_MAX);
                    wait
                }
            };
            // Sleep until woken, asked, or the time is up; a lock ends the wait.
            let deadline = tokio::time::Instant::now() + wait;
            loop {
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                if remaining.is_zero() {
                    break;
                }
                tokio::select! {
                    () = self.sync.wake.notified() => {
                        if self.sync.now.swap(false, Ordering::SeqCst) {
                            backoff = BACKOFF_START;
                        } else if result.is_err() {
                            // Something changed, but the server is out of
                            // reach: keep waiting out the backoff.
                            continue;
                        } else {
                            // Let a burst of changes settle, then send once.
                            tokio::time::sleep(SETTLE).await;
                        }
                        break;
                    }
                    () = tokio::time::sleep(remaining.min(WATCH)) => {
                        if !self.signed_in_and_unlocked() {
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Keeps the events connection open while the vault is, and wakes the
    /// scheduler on each "changed".
    async fn listen(self: std::sync::Arc<Self>) {
        loop {
            while !self.signed_in_and_unlocked() {
                tokio::time::sleep(WATCH).await;
            }
            match self.api().open_events().await {
                Ok(mut stream) => {
                    // Anything that changed while there was no connection.
                    self.sync.nudge();
                    loop {
                        match tokio::time::timeout(Duration::from_secs(5), stream.next()).await {
                            Ok(Some(())) => self.sync.nudge(),
                            Ok(None) => break,
                            // Quiet: check the vault is still open.
                            Err(_) => {
                                if !self.signed_in_and_unlocked() {
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(error) => tracing::debug!(%error, "could not open the events connection"),
            }
            tokio::time::sleep(RECONNECT).await;
        }
    }
}

impl KandadoApp {
    /// Sets where syncing stands, so a test can draw the banner for a state
    /// without a server that is really down.
    #[doc(hidden)]
    pub fn pretend_sync_state(&self, state: SyncState) {
        self.sync.set(state, false);
    }
}
