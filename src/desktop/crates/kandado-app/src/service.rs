//! The application service the host talks to. One object that owns the API
//! client, the local store, the unlocked session and the settings, and exposes
//! each user action as a method. Views never touch keys: they pass in what the
//! person typed and get back view data.
//!
//! The flows are the ones in `docs/architecture/security.md`:
//! first sign-in, new device, forgot password, unlock, change password.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

use kandado_crypto::{
    SecretKey,
    account::{self, AccountBundle, MasterRewrap, NewAccount},
    kdf::KdfParams,
    recovery::RecoveryPhrase,
};
use kandado_model::strength;
use kandado_protocol::{
    AccountKeys, ChangeMaster, DeviceInfo, DeviceList, FlowPurpose, KdfSettings, Opaque,
    ProblemCode,
};
use kandado_vault::{
    Api, ApiError, Session, Store, StoreError, Tokens, VaultError, store::LocalAccount, vaults,
};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    auth::{AuthError, PendingSignIn},
    clipboard::Clipboard,
    device_key::{DeviceKey, DeviceKeyError},
    lock::{Attempt, IdleTimer, LockReason, UnlockGuard},
    secrets::SecretStore,
    settings::{Settings, SettingsError},
};

const REFRESH_TOKEN: &str = "refresh-token";

/// The shortest master password accepted.
pub const MIN_PASSWORD_LEN: usize = 8;
/// The lowest strength score accepted (2 is "Fair").
pub const MIN_PASSWORD_SCORE: u8 = 2;

/// Where the app is, which decides the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// No account on this computer. Shows D-01.
    SignedOut,
    /// Signed in with Google but no keys yet. Shows D-02.
    NeedsMasterPassword {
        email: String,
    },
    /// Keys are here but the vault is locked. Shows D-04.
    Locked {
        email: String,
        hello: bool,
    },
    Unlocked {
        email: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Settings(#[from] SettingsError),
    #[error("a key operation failed: {0}")]
    Crypto(#[from] kandado_crypto::Error),
    #[error(transparent)]
    DeviceKey(#[from] DeviceKeyError),
    #[error(transparent)]
    Vault(#[from] VaultError),
    #[error("the clipboard could not be used: {0}")]
    Clipboard(String),
    #[error("choose a longer, less guessable master password")]
    WeakPassword,
    #[error("that is not the recovery phrase")]
    WrongPhrase,
    #[error("that is not your master password")]
    WrongPassword,
    #[error("the app is not in the right state for that")]
    WrongState,
    #[error("this computer's secret store failed: {0}")]
    Secrets(String),
}

/// Why unlocking did not happen.
#[derive(Debug)]
pub enum UnlockError {
    /// The password is wrong. `tries_left` is how many remain before the wait.
    WrongPassword {
        tries_left: u32,
    },
    /// Too many wrong passwords. Try again after this long.
    Wait(std::time::Duration),
    /// Hello was declined or is not set up. The password still works.
    HelloUnavailable,
    Failed(AppError),
}

/// Whether a sensitive action went through or needs a Google sign-in first.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Done,
    /// Call [`KandadoApp::begin_google`], open its URL, then
    /// [`KandadoApp::finish_google`], then [`KandadoApp::resume`].
    NeedsGoogle,
}

enum Pending {
    None,
    /// The keys exist only in memory until the recovery phrase is confirmed.
    NewAccount {
        account: Box<NewAccount>,
        email: String,
    },
    /// A master password change waiting for a fresh Google sign-in.
    MasterChange {
        change: ChangeMaster,
        rewrap: MasterRewrap,
        /// Set when this is a recovery: unlock with it once saved.
        unlock_with: Option<SecretKey>,
    },
}

pub struct Paths {
    pub data_dir: PathBuf,
}

impl Paths {
    pub fn vault_db(&self) -> PathBuf {
        self.data_dir.join("vault.db")
    }

    pub fn settings_json(&self) -> PathBuf {
        self.data_dir.join("settings.json")
    }
}

pub struct KandadoApp {
    paths: Paths,
    settings: Mutex<Settings>,
    api: Api,
    store: Mutex<Option<Store>>,
    session: Mutex<Session>,
    secrets: Arc<dyn SecretStore>,
    device_key: Arc<dyn DeviceKey>,
    clipboard: Clipboard,
    guard: Mutex<UnlockGuard>,
    idle: Mutex<IdleTimer>,
    pub(crate) sync: crate::sync::SyncControl,
    pub(crate) last_import: Mutex<Option<kandado_vault::import::ImportReport>>,
    pending: Mutex<Pending>,
    kdf: KdfParams,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().expect("a panic poisoned the app state")
}

fn secrets_error(error: crate::secrets::SecretsError) -> AppError {
    AppError::Secrets(error.to_string())
}

fn device_info() -> DeviceInfo {
    DeviceInfo {
        name: std::env::var("COMPUTERNAME").unwrap_or_else(|_| "This computer".to_owned()),
        platform: "Windows".to_owned(),
    }
}

fn key_settings(params: KdfParams) -> KdfSettings {
    KdfSettings {
        memory_kib: params.memory_kib,
        iterations: params.iterations,
        parallelism: params.parallelism,
    }
}

fn to_keys(bundle: &AccountBundle) -> AccountKeys {
    AccountKeys {
        version: bundle.version,
        kdf_salt: Opaque::new(bundle.kdf_salt.to_vec()),
        kdf: key_settings(bundle.kdf_params),
        user_key_by_master: Opaque::new(bundle.user_key_by_master.clone()),
        user_key_by_recovery: Opaque::new(bundle.user_key_by_recovery.clone()),
        public_x25519: Opaque::new(bundle.public_x25519.to_vec()),
        private_keys_enc: Opaque::new(bundle.private_keys_enc.clone()),
    }
}

fn from_keys(keys: &AccountKeys) -> Result<AccountBundle, AppError> {
    let invalid = || AppError::Crypto(kandado_crypto::Error::Format);
    Ok(AccountBundle {
        version: keys.version,
        kdf_salt: keys.kdf_salt.as_bytes().try_into().map_err(|_| invalid())?,
        kdf_params: KdfParams {
            memory_kib: keys.kdf.memory_kib,
            iterations: keys.kdf.iterations,
            parallelism: keys.kdf.parallelism,
        },
        user_key_by_master: keys.user_key_by_master.as_bytes().to_vec(),
        user_key_by_recovery: keys.user_key_by_recovery.as_bytes().to_vec(),
        public_x25519: keys
            .public_x25519
            .as_bytes()
            .try_into()
            .map_err(|_| invalid())?,
        private_keys_enc: keys.private_keys_enc.as_bytes().to_vec(),
    })
}

/// Runs a slow key derivation off the async threads.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(work)
        .await
        .expect("key derivation does not panic")
}

/// The strength rules for a new master password. `email` is fed to the
/// estimator so a password built from it scores lower.
pub fn check_new_password(password: &str, email: &str) -> Result<strength::Strength, AppError> {
    let estimate = strength::estimate(password, &[email]);
    if password.chars().count() < MIN_PASSWORD_LEN || estimate.score < MIN_PASSWORD_SCORE {
        return Err(AppError::WeakPassword);
    }
    Ok(estimate)
}

impl KandadoApp {
    pub fn new(
        paths: Paths,
        secrets: Arc<dyn SecretStore>,
        device_key: Arc<dyn DeviceKey>,
    ) -> Result<Arc<Self>, AppError> {
        Self::with_params(paths, secrets, device_key, KdfParams::DEFAULT, None)
    }

    /// Like [`KandadoApp::new`] with chosen key derivation cost and server
    /// address. Tests use the cheapest allowed cost.
    pub fn with_params(
        paths: Paths,
        secrets: Arc<dyn SecretStore>,
        device_key: Arc<dyn DeviceKey>,
        kdf: KdfParams,
        server_url: Option<&str>,
    ) -> Result<Arc<Self>, AppError> {
        Self::with_clipboard(
            paths,
            secrets,
            device_key,
            Clipboard::system(),
            kdf,
            server_url,
        )
    }

    /// Like [`KandadoApp::with_params`] with a chosen clipboard. Tests pass
    /// one that lives in memory, so they never touch the real one.
    pub fn with_clipboard(
        paths: Paths,
        secrets: Arc<dyn SecretStore>,
        device_key: Arc<dyn DeviceKey>,
        clipboard: Clipboard,
        kdf: KdfParams,
        server_url: Option<&str>,
    ) -> Result<Arc<Self>, AppError> {
        let settings = Settings::load(&paths.settings_json());
        let store = Store::open(&paths.vault_db())?;
        let remember = secrets.clone();
        let base = server_url.unwrap_or(&settings.server_url).to_owned();
        let api = Api::new(&base, move |tokens: &Tokens| {
            if let Err(error) = remember.save(REFRESH_TOKEN, tokens.refresh.as_bytes()) {
                tracing::error!(%error, "could not store the refresh token");
            }
        })?;
        Ok(Arc::new(Self {
            paths,
            settings: Mutex::new(settings),
            api,
            store: Mutex::new(Some(store)),
            session: Mutex::new(Session::new()),
            secrets,
            device_key,
            clipboard,
            guard: Mutex::new(UnlockGuard::default()),
            idle: Mutex::new(IdleTimer::new(Instant::now())),
            sync: crate::sync::SyncControl::new(),
            last_import: Mutex::new(None),
            pending: Mutex::new(Pending::None),
            kdf,
        }))
    }

    pub fn api(&self) -> &Api {
        &self.api
    }

    // ---- settings ------------------------------------------------------

    pub fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    /// Changes settings and saves them at once.
    pub fn update_settings(&self, change: impl FnOnce(&mut Settings)) -> Result<(), AppError> {
        let mut settings = lock(&self.settings);
        change(&mut settings);
        settings.save(&self.paths.settings_json())?;
        Ok(())
    }

    // ---- state ---------------------------------------------------------

    fn local_account(&self) -> Result<Option<LocalAccount>, AppError> {
        match lock(&self.store).as_ref() {
            Some(store) => Ok(store.account()?),
            None => Ok(None),
        }
    }

    fn with_store<T>(
        &self,
        work: impl FnOnce(&Store) -> Result<T, StoreError>,
    ) -> Result<T, AppError> {
        let guard = lock(&self.store);
        let store = guard.as_ref().ok_or(AppError::WrongState)?;
        Ok(work(store)?)
    }

    /// Runs `work` with the store and the session. The store lock is always
    /// taken before the session lock.
    pub(crate) fn with_store_and_session<T>(
        &self,
        work: impl FnOnce(&Store, &mut Session) -> Result<T, VaultError>,
    ) -> Result<T, AppError> {
        let guard = lock(&self.store);
        let store = guard.as_ref().ok_or(AppError::WrongState)?;
        Ok(work(store, &mut lock(&self.session))?)
    }

    /// Like [`KandadoApp::with_store_and_session`], with the vault's own
    /// error, for the sync engine.
    pub(crate) fn raw_vault<T>(
        &self,
        work: impl FnOnce(&Store, &mut Session) -> Result<T, VaultError>,
    ) -> Result<T, VaultError> {
        let guard = lock(&self.store);
        let store = guard.as_ref().ok_or(VaultError::Locked)?;
        work(store, &mut lock(&self.session))
    }

    pub(crate) fn with_session<T>(&self, read: impl FnOnce(&Session) -> T) -> T {
        read(&lock(&self.session))
    }

    pub(crate) fn clipboard(&self) -> &Clipboard {
        &self.clipboard
    }

    pub fn is_unlocked(&self) -> bool {
        lock(&self.session).is_unlocked()
    }

    /// Works out the screen to show at startup.
    pub async fn restore(&self) -> Result<Phase, AppError> {
        if let Some(account) = self.local_account()? {
            // Reconnect the session if we have a refresh token. Offline is fine.
            if let Some(refresh) = self.secrets.load(REFRESH_TOKEN).map_err(secrets_error)? {
                self.api
                    .set_tokens(Some(Tokens {
                        access: Zeroizing::new(String::new()),
                        refresh: Zeroizing::new(String::from_utf8_lossy(&refresh).into_owned()),
                    }))
                    .await;
            }
            return Ok(Phase::Locked {
                email: account.email,
                hello: self.hello_ready(),
            });
        }
        // No local account. A refresh token means sign-in finished but the
        // master password was not set (the app was closed in between).
        if self
            .secrets
            .load(REFRESH_TOKEN)
            .map_err(secrets_error)?
            .is_some()
        {
            self.reconnect().await?;
            return self.after_sign_in().await;
        }
        Ok(Phase::SignedOut)
    }

    async fn reconnect(&self) -> Result<(), AppError> {
        let refresh = self
            .secrets
            .load(REFRESH_TOKEN)
            .map_err(secrets_error)?
            .ok_or(AppError::WrongState)?;
        self.api
            .set_tokens(Some(Tokens {
                access: Zeroizing::new(String::new()),
                refresh: Zeroizing::new(String::from_utf8_lossy(&refresh).into_owned()),
            }))
            .await;
        Ok(())
    }

    fn hello_ready(&self) -> bool {
        self.settings().windows_hello
            && self
                .with_store(|store| store.device_wrap())
                .ok()
                .flatten()
                .is_some()
    }

    // ---- sign-in (D-01) --------------------------------------------------

    /// Starts a Google flow. With a session it is a step-up; without, a sign-in.
    pub async fn begin_google(&self) -> Result<PendingSignIn, AppError> {
        let access = self
            .api
            .access_token()
            .await
            .filter(|token| !token.is_empty());
        let purpose = if access.is_some() {
            FlowPurpose::StepUp
        } else {
            FlowPurpose::SignIn
        };
        Ok(PendingSignIn::start(&self.api, purpose, access.as_deref()).await?)
    }

    /// Waits for the browser and completes the flow. After a sign-in returns
    /// the next [`Phase`]; after a step-up returns the phase unchanged.
    pub async fn finish_google(&self, pending: PendingSignIn) -> Result<Option<Phase>, AppError> {
        let purpose = pending.purpose;
        let (code, verifier) = pending.wait().await?;
        match purpose {
            FlowPurpose::SignIn => {
                self.api.sign_in(&code, &verifier, device_info()).await?;
                Ok(Some(self.after_sign_in().await?))
            }
            FlowPurpose::StepUp => {
                self.api.confirm(&code, &verifier).await?;
                Ok(None)
            }
        }
    }

    /// After Google: new account, or an existing one on a new device.
    async fn after_sign_in(&self) -> Result<Phase, AppError> {
        let me = self.api.me().await?;
        // This computer already belongs to one account. Another Google account
        // must not replace its keys: that would drop the vault copy here.
        if let Some(existing) = self.local_account()?
            && existing.user_id != me.id
        {
            self.api.logout().await;
            self.secrets.delete(REFRESH_TOKEN).map_err(secrets_error)?;
            return Err(AuthError::WrongAccount.into());
        }
        if !me.has_keys {
            return Ok(Phase::NeedsMasterPassword { email: me.email });
        }
        let keys = self.api.keys().await?;
        let account = LocalAccount {
            user_id: me.id,
            device_id: me.device_id,
            email: me.email.clone(),
            display_name: me.display_name,
            bundle: from_keys(&keys)?,
        };
        self.with_store(|store| store.save_account(&account))?;
        // An account that already exists has vaults on the server: this
        // computer waits for the first pull instead of making a Personal one.
        self.with_store(|store| {
            if !store.has_synced()? {
                store.set_cursor(0)?;
            }
            Ok(())
        })?;
        Ok(Phase::Locked {
            email: me.email,
            hello: false,
        })
    }

    // ---- create master password (D-02, D-03) -----------------------------

    /// Derives the keys for a new account and returns the recovery phrase to
    /// show. Nothing is uploaded until [`KandadoApp::confirm_recovery_saved`],
    /// so closing the app at D-03 loses nothing.
    pub async fn create_master_password(
        &self,
        password: String,
    ) -> Result<Vec<&'static str>, AppError> {
        let password = Zeroizing::new(password);
        let me = self.api.me().await?;
        check_new_password(&password, &me.email)?;
        let (user, kdf) = (*me.id.as_bytes(), self.kdf);
        let account = blocking(move || account::create(user, &password, kdf)).await?;
        let words = account.recovery.words().collect();
        *lock(&self.pending) = Pending::NewAccount {
            account: Box::new(account),
            email: me.email,
        };
        Ok(words)
    }

    /// The recovery phrase to show or save, while D-03 is open.
    pub fn pending_phrase(&self) -> Option<Vec<&'static str>> {
        match &*lock(&self.pending) {
            Pending::NewAccount { account, .. } => Some(account.recovery.words().collect()),
            _ => None,
        }
    }

    /// The person confirmed they saved the phrase: upload the keys, keep a
    /// copy on this computer and unlock.
    pub async fn confirm_recovery_saved(&self) -> Result<Phase, AppError> {
        let (account, email) = match std::mem::replace(&mut *lock(&self.pending), Pending::None) {
            Pending::NewAccount { account, email } => (account, email),
            other => {
                *lock(&self.pending) = other;
                return Err(AppError::WrongState);
            }
        };
        let me = self.api.me().await?;
        let keys = to_keys(&account.bundle);
        if let Err(error) = self.api.create_keys(&keys).await {
            // Keep the keys so the person can try again.
            *lock(&self.pending) = Pending::NewAccount { account, email };
            return Err(error.into());
        }
        let local = LocalAccount {
            user_id: me.id,
            device_id: me.device_id,
            email: email.clone(),
            display_name: me.display_name,
            bundle: account.bundle.clone(),
        };
        self.with_store(|store| store.save_account(&local))?;
        self.finish_unlock(account.user_key.clone())?;
        Ok(Phase::Unlocked { email })
    }

    // ---- unlock (D-04) -----------------------------------------------------

    /// Tries the master password. Counts wrong tries; five in a row make the
    /// next try wait a minute.
    pub async fn unlock_with_password(&self, password: String) -> Result<Phase, UnlockError> {
        let password = Zeroizing::new(password);
        if let Attempt::Wait(wait) = lock(&self.guard).check(Instant::now()) {
            return Err(UnlockError::Wait(wait));
        }
        let account = self
            .local_account()
            .map_err(UnlockError::Failed)?
            .ok_or(UnlockError::Failed(AppError::WrongState))?;
        let (user, bundle) = (*account.user_id.as_bytes(), account.bundle.clone());
        let result =
            blocking(move || account::unlock_with_password(user, &bundle, &password)).await;
        match result {
            Ok(user_key) => {
                lock(&self.guard).succeeded();
                self.finish_unlock(user_key).map_err(UnlockError::Failed)?;
                Ok(Phase::Unlocked {
                    email: account.email,
                })
            }
            Err(kandado_crypto::Error::Decrypt) => {
                let tries_left = lock(&self.guard).failed(Instant::now());
                Err(UnlockError::WrongPassword { tries_left })
            }
            Err(error) => Err(UnlockError::Failed(error.into())),
        }
    }

    /// Unlocks with Windows Hello. Declining is not a failed password try.
    pub async fn unlock_with_hello(&self) -> Result<Phase, UnlockError> {
        let account = self
            .local_account()
            .map_err(UnlockError::Failed)?
            .ok_or(UnlockError::Failed(AppError::WrongState))?;
        let wrap = self
            .with_store(|store| store.device_wrap())
            .map_err(UnlockError::Failed)?
            .ok_or(UnlockError::HelloUnavailable)?;
        let device_key = self.device_key.clone();
        let (user, device) = (account.user_id, account.device_id);
        let result = blocking(move || device_key.recover(user, device, &wrap)).await;
        match result {
            Ok(user_key) => {
                self.finish_unlock(user_key).map_err(UnlockError::Failed)?;
                Ok(Phase::Unlocked {
                    email: account.email,
                })
            }
            Err(DeviceKeyError::Declined | DeviceKeyError::Unavailable) => {
                Err(UnlockError::HelloUnavailable)
            }
            Err(error) => Err(UnlockError::Failed(error.into())),
        }
    }

    /// Holds the user key and opens the vaults: unseals their keys and builds
    /// the search index. If that fails the app stays locked.
    fn finish_unlock(&self, user_key: SecretKey) -> Result<(), AppError> {
        let account = self.local_account()?.ok_or(AppError::WrongState)?;
        let opened = self.with_store_and_session(|store, session| {
            session.unlock(user_key);
            vaults::open(store, session, &account, crate::vault::now())
        });
        if opened.is_err() {
            lock(&self.session).lock();
        }
        opened?;
        lock(&self.idle).touch(Instant::now());
        // Pull what other computers did, and send what waited offline.
        self.sync.nudge();
        Ok(())
    }

    // ---- locking -----------------------------------------------------------

    pub fn lock(&self, reason: LockReason) {
        tracing::info!(?reason, "locking");
        lock(&self.session).lock();
        // A copied password does not outlive the lock.
        self.clipboard.clear_now();
    }

    /// Call on any key press or click while unlocked.
    pub fn touch(&self) {
        lock(&self.idle).touch(Instant::now());
    }

    /// Locks if the idle time has run out. Returns true if it locked.
    pub fn lock_if_idle(&self) -> bool {
        if !self.is_unlocked() {
            return false;
        }
        let setting = self.settings().lock_after;
        if lock(&self.idle).expired(setting, Instant::now()) {
            self.lock(LockReason::Idle);
            return true;
        }
        false
    }

    /// Locks for sleep or a session switch when the setting asks for it.
    pub fn lock_for_system_event(&self, reason: LockReason) -> bool {
        if self.settings().lock_on_sleep && self.is_unlocked() {
            self.lock(reason);
            return true;
        }
        false
    }

    // ---- forgot password / change password ------------------------------------

    /// Forgot-password flow: the recovery phrase unwraps the user key, which
    /// is wrapped again under a new master password. Changing the password on
    /// the server needs a fresh Google sign-in.
    pub async fn recover(&self, phrase: String, new_password: String) -> Result<Outcome, AppError> {
        let phrase = Zeroizing::new(phrase);
        let new_password = Zeroizing::new(new_password);
        let account = self.local_account()?.ok_or(AppError::WrongState)?;
        check_new_password(&new_password, &account.email)?;
        let phrase = RecoveryPhrase::parse(&phrase).map_err(|_| AppError::WrongPhrase)?;
        let (user, bundle, kdf) = (
            *account.user_id.as_bytes(),
            account.bundle.clone(),
            self.kdf,
        );
        let (user_key, rewrap) = blocking(move || {
            let user_key = account::unlock_with_recovery(user, &bundle, &phrase)?;
            let rewrap = account::rewrap_master(user, &user_key, &new_password, kdf)?;
            Ok::<_, kandado_crypto::Error>((user_key, rewrap))
        })
        .await
        .map_err(|error| match error {
            kandado_crypto::Error::Decrypt => AppError::WrongPhrase,
            other => other.into(),
        })?;
        self.submit_master_change(rewrap, Some(user_key)).await
    }

    /// Changes the master password while unlocked (D-50). Needs the current
    /// password, then a fresh Google sign-in.
    pub async fn change_master_password(
        &self,
        current: String,
        new_password: String,
    ) -> Result<Outcome, AppError> {
        let current = Zeroizing::new(current);
        let new_password = Zeroizing::new(new_password);
        let account = self.local_account()?.ok_or(AppError::WrongState)?;
        check_new_password(&new_password, &account.email)?;
        let (user, bundle, kdf) = (
            *account.user_id.as_bytes(),
            account.bundle.clone(),
            self.kdf,
        );
        let rewrap = blocking(move || {
            let user_key = account::unlock_with_password(user, &bundle, &current)?;
            account::rewrap_master(user, &user_key, &new_password, kdf)
        })
        .await
        .map_err(|error| match error {
            kandado_crypto::Error::Decrypt => AppError::WrongPassword,
            other => other.into(),
        })?;
        self.submit_master_change(rewrap, None).await
    }

    async fn submit_master_change(
        &self,
        rewrap: MasterRewrap,
        unlock_with: Option<SecretKey>,
    ) -> Result<Outcome, AppError> {
        let change = ChangeMaster {
            kdf_salt: Opaque::new(rewrap.kdf_salt.to_vec()),
            kdf: key_settings(rewrap.kdf_params),
            user_key_by_master: Opaque::new(rewrap.user_key_by_master.clone()),
        };
        match self.api.change_master(&change).await {
            Ok(()) => {
                self.apply_master_change(&rewrap, unlock_with)?;
                Ok(Outcome::Done)
            }
            Err(error) if needs_google(&error) => {
                *lock(&self.pending) = Pending::MasterChange {
                    change,
                    rewrap,
                    unlock_with,
                };
                Ok(Outcome::NeedsGoogle)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn apply_master_change(
        &self,
        rewrap: &MasterRewrap,
        unlock_with: Option<SecretKey>,
    ) -> Result<(), AppError> {
        let mut account = self.local_account()?.ok_or(AppError::WrongState)?;
        account.bundle.kdf_salt = rewrap.kdf_salt;
        account.bundle.kdf_params = rewrap.kdf_params;
        account.bundle.user_key_by_master = rewrap.user_key_by_master.clone();
        self.with_store(|store| store.save_account(&account))?;
        if let Some(user_key) = unlock_with {
            lock(&self.guard).succeeded();
            self.finish_unlock(user_key)?;
        }
        Ok(())
    }

    /// Carries on with whatever was waiting for a Google sign-in.
    pub async fn resume(&self) -> Result<Outcome, AppError> {
        let pending = std::mem::replace(&mut *lock(&self.pending), Pending::None);
        match pending {
            Pending::MasterChange {
                change,
                rewrap,
                unlock_with,
            } => match self.api.change_master(&change).await {
                Ok(()) => {
                    self.apply_master_change(&rewrap, unlock_with)?;
                    Ok(Outcome::Done)
                }
                Err(error) if needs_google(&error) => {
                    *lock(&self.pending) = Pending::MasterChange {
                        change,
                        rewrap,
                        unlock_with,
                    };
                    Ok(Outcome::NeedsGoogle)
                }
                Err(error) => Err(error.into()),
            },
            other => {
                *lock(&self.pending) = other;
                Err(AppError::WrongState)
            }
        }
    }

    // ---- Windows Hello (D-44) ------------------------------------------------

    /// Turns Windows Hello unlock on. Needs the vault unlocked.
    pub async fn enable_hello(&self) -> Result<(), AppError> {
        let account = self.local_account()?.ok_or(AppError::WrongState)?;
        let user_key = lock(&self.session)
            .user_key()
            .cloned()
            .ok_or(AppError::WrongState)?;
        let device_key = self.device_key.clone();
        let (user, device) = (account.user_id, account.device_id);
        let wrap = blocking(move || device_key.protect(user, device, &user_key)).await?;
        self.with_store(|store| store.save_device_wrap(&wrap))?;
        self.update_settings(|settings| settings.windows_hello = true)
    }

    pub fn disable_hello(&self) -> Result<(), AppError> {
        self.with_store(|store| store.clear_device_wrap())?;
        self.device_key.forget();
        self.update_settings(|settings| settings.windows_hello = false)
    }

    pub fn hello_available(&self) -> bool {
        self.device_key.available()
    }

    // ---- recovery phrase file ---------------------------------------------------

    /// The text of the "Save as file" download at D-03.
    pub fn recovery_file_text(words: &[&str], email: &str) -> String {
        let numbered: Vec<String> = words
            .iter()
            .enumerate()
            .map(|(index, word)| format!("{:>2}. {word}", index + 1))
            .collect();
        format!(
            "Kandado recovery phrase\r\nAccount: {email}\r\n\r\n{}\r\n\r\n\
             These 12 words are the only way back into your vault if you forget your master password.\r\n\
             Kandado cannot reset it for you. Keep this file somewhere safe and private.\r\n",
            numbered.join("\r\n")
        )
    }

    // ---- devices (D-49) and account (D-50) -----------------------------------------

    pub async fn devices(&self) -> Result<DeviceList, AppError> {
        Ok(self.api.devices().await?)
    }

    pub async fn revoke_device(&self, id: Uuid) -> Result<(), AppError> {
        Ok(self.api.revoke_device(id).await?)
    }

    /// Signs out every other device. Needs a fresh Google sign-in.
    pub async fn revoke_other_devices(&self) -> Result<Outcome, AppError> {
        match self.api.revoke_other_devices().await {
            Ok(()) => Ok(Outcome::Done),
            Err(error) if needs_google(&error) => Ok(Outcome::NeedsGoogle),
            Err(error) => Err(error.into()),
        }
    }

    /// Deletes the account on the server, then everything on this computer.
    /// Needs a fresh Google sign-in.
    pub async fn delete_account(&self) -> Result<Outcome, AppError> {
        match self.api.delete_account().await {
            Ok(()) => {
                self.forget_everything()?;
                Ok(Outcome::Done)
            }
            Err(error) if needs_google(&error) => Ok(Outcome::NeedsGoogle),
            Err(error) => Err(error.into()),
        }
    }

    /// Checks the master password without changing anything. Used before
    /// deleting the account. Counts toward the same wrong-password limit.
    pub async fn verify_password(&self, password: String) -> Result<(), AppError> {
        let password = Zeroizing::new(password);
        if let Attempt::Wait(_) = lock(&self.guard).check(Instant::now()) {
            return Err(AppError::WrongPassword);
        }
        let account = self.local_account()?.ok_or(AppError::WrongState)?;
        let (user, bundle) = (*account.user_id.as_bytes(), account.bundle.clone());
        match blocking(move || account::unlock_with_password(user, &bundle, &password)).await {
            Ok(_) => {
                lock(&self.guard).succeeded();
                Ok(())
            }
            Err(kandado_crypto::Error::Decrypt) => {
                lock(&self.guard).failed(Instant::now());
                Err(AppError::WrongPassword)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// The account as the server has it (plan, display name).
    pub async fn profile(&self) -> Result<kandado_protocol::Me, AppError> {
        Ok(self.api.me().await?)
    }

    /// The email of the signed-in account, from this computer's copy.
    pub fn email(&self) -> Option<String> {
        self.local_account()
            .ok()
            .flatten()
            .map(|account| account.email)
    }

    /// The signed-in account's id, from this computer's copy.
    pub fn user_id(&self) -> Option<Uuid> {
        self.local_account()
            .ok()
            .flatten()
            .map(|account| account.user_id)
    }

    /// The user key sealed to a paired browser extension (D-45, see
    /// `kandado_crypto::pairing`), or `None` while locked or signed out.
    pub fn seal_user_key_for_extension(
        &self,
        pairing: Uuid,
        public_key: &[u8; 32],
    ) -> Result<Option<Vec<u8>>, AppError> {
        let Some(user) = self.user_id() else {
            return Ok(None);
        };
        let session = lock(&self.session);
        let Some(user_key) = session.user_key() else {
            return Ok(None);
        };
        Ok(Some(kandado_crypto::pairing::seal_user_key(
            *user.as_bytes(),
            *pairing.as_bytes(),
            &kandado_crypto::seal::PublicKey::from_bytes(*public_key),
            user_key,
        )?))
    }

    /// The id of this computer's session, to mark "This device" at D-49.
    pub fn device_id(&self) -> Option<Uuid> {
        self.local_account()
            .ok()
            .flatten()
            .map(|account| account.device_id)
    }

    // ---- sign out (D-52) --------------------------------------------------------------

    /// Ends the session on the server (if reachable), then removes everything
    /// this computer holds: the vault file, the refresh token, the device key.
    pub async fn sign_out(&self) -> Result<(), AppError> {
        self.api.logout().await;
        self.forget_everything()
    }

    fn forget_everything(&self) -> Result<(), AppError> {
        lock(&self.session).lock();
        self.sync.reset();
        self.clipboard.clear_now();
        *lock(&self.pending) = Pending::None;
        self.device_key.forget();
        self.secrets.delete(REFRESH_TOKEN).map_err(secrets_error)?;
        if let Some(store) = lock(&self.store).take() {
            store.wipe()?;
        }
        // Start an empty store, so a new sign-in can begin without a restart.
        *lock(&self.store) = Some(Store::open(&self.paths.vault_db())?);
        *lock(&self.guard) = UnlockGuard::default();
        self.update_settings(|settings| settings.windows_hello = false)?;
        Ok(())
    }
}

fn needs_google(error: &ApiError) -> bool {
    matches!(error, ApiError::SignedOut) || error.code() == Some(ProblemCode::StepUpRequired)
}

impl KandadoApp {
    /// The key-derivation cost for new keys and encrypted exports.
    pub(crate) fn kdf_params(&self) -> KdfParams {
        self.kdf
    }
}
