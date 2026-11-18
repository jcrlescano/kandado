//! Import and export for the screens (D-05, D-47, D-48).
//!
//! Reading a file, importing it and exporting the vault all take a while, so
//! they run on blocking threads and report through [`ImportProgress`], which a
//! screen polls to draw the bar and to cancel. A [`Preview`] is what a screen
//! shows about a file: counts and reasons, never a secret. The [`Plan`] behind
//! it holds the secrets and is kept only until the import is done or closed.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use kandado_vault::import::{
    self, ExportFormat, ImportError, ImportReport, ImportTarget, Plan, Skipped, Source,
};
use uuid::Uuid;

use crate::{
    service::{AppError, KandadoApp},
    vault::now,
};

/// The largest file read for an import. Bigger than any real export.
const MAX_FILE: u64 = 1024 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error(transparent)]
    Import(#[from] ImportError),
    #[error("the file could not be read or written: {0}")]
    Io(String),
    #[error("that file is too large to import")]
    TooLarge,
    #[error("the import was cancelled")]
    Cancelled,
    #[error(transparent)]
    App(#[from] AppError),
    /// The account password is wrong, or too many wrong tries were made.
    #[error("that is not your account password")]
    WrongPassword,
}

/// What a screen shows about a file that was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub source: Source,
    pub file_name: String,
    pub size: u64,
    /// Items in the file, importable or not.
    pub found: usize,
    pub importable: usize,
    /// Of the importable ones, how many were in the source's trash.
    pub trashed: usize,
    /// Email aliases that will be kept as notes.
    pub aliases: usize,
    /// The source's vaults, if it has any.
    pub vaults: Vec<String>,
    pub skipped: Vec<Skipped>,
    pub ignored: Vec<String>,
}

/// A file that was read: what to show, and what to import.
pub struct Reading {
    pub preview: Preview,
    pub plan: Arc<Plan>,
}

/// Where imported items go, as a screen chooses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportChoice {
    Into(Uuid),
    /// A vault per source vault, by name; the rest into the first vault.
    MatchSource,
}

/// How far an import is, and a way to stop it. Share one between the screen
/// and the import.
#[derive(Default)]
pub struct ImportProgress {
    done: AtomicUsize,
    total: AtomicUsize,
    cancel: AtomicBool,
}

impl ImportProgress {
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    /// `(done, total)`. Total is 0 until the import starts.
    pub fn get(&self) -> (usize, usize) {
        (
            self.done.load(Ordering::SeqCst),
            self.total.load(Ordering::SeqCst),
        )
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

/// What an export did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSummary {
    pub path: PathBuf,
    pub items: usize,
    /// Items the format cannot hold (CSV leaves out cards and identities).
    pub omitted: usize,
}

fn io_error(error: std::io::Error) -> TransferError {
    TransferError::Io(error.to_string())
}

impl KandadoApp {
    /// Reads an export file into a preview and a plan.
    pub async fn read_import(
        &self,
        source: Source,
        path: &Path,
        passphrase: Option<String>,
    ) -> Result<Reading, TransferError> {
        let metadata = tokio::fs::metadata(path).await.map_err(io_error)?;
        if metadata.len() > MAX_FILE {
            return Err(TransferError::TooLarge);
        }
        let bytes = zeroize::Zeroizing::new(tokio::fs::read(path).await.map_err(io_error)?);
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.read_import_bytes(source, bytes, file_name, passphrase)
            .await
    }

    /// Like [`KandadoApp::read_import`] for a file already in memory.
    pub async fn read_import_bytes(
        &self,
        source: Source,
        bytes: zeroize::Zeroizing<Vec<u8>>,
        file_name: String,
        passphrase: Option<String>,
    ) -> Result<Reading, TransferError> {
        let size = bytes.len() as u64;
        let passphrase = passphrase.map(zeroize::Zeroizing::new);
        let plan = tokio::task::spawn_blocking(move || {
            import::read(source, &bytes, passphrase.as_deref().map(String::as_str))
        })
        .await
        .map_err(|_| TransferError::Io("the file could not be read".into()))??;
        let preview = Preview {
            source,
            file_name,
            size,
            found: plan.found(),
            importable: plan.items.len(),
            trashed: plan.trashed(),
            aliases: plan.aliases(),
            vaults: plan.vault_names().into_iter().map(str::to_owned).collect(),
            skipped: plan.skipped.clone(),
            ignored: plan.ignored.clone(),
        };
        Ok(Reading {
            preview,
            plan: Arc::new(plan),
        })
    }

    /// Imports a plan, all of it or none. Reports on `progress`.
    pub async fn run_import(
        self: &Arc<Self>,
        plan: Arc<Plan>,
        choice: ImportChoice,
        include_trashed: bool,
        progress: Arc<ImportProgress>,
    ) -> Result<ImportReport, TransferError> {
        let app = self.clone();
        let result = tokio::task::spawn_blocking(move || {
            app.raw_vault(|store, session| {
                let fallback = crate::vault::first_vault(store, session)?;
                let target = match choice {
                    ImportChoice::Into(vault) => ImportTarget::Into(vault),
                    ImportChoice::MatchSource => ImportTarget::BySource { fallback },
                };
                let total = plan
                    .items
                    .iter()
                    .filter(|item| include_trashed || !item.trashed)
                    .count();
                progress.total.store(total, Ordering::SeqCst);
                progress.done.store(0, Ordering::SeqCst);
                import::apply(
                    store,
                    session,
                    &plan,
                    target,
                    include_trashed,
                    now(),
                    &mut |step| {
                        progress.done.store(step.done, Ordering::SeqCst);
                        !progress.cancel.load(Ordering::SeqCst)
                    },
                )
            })
        })
        .await
        .map_err(|_| TransferError::Io("the import stopped".into()))?;
        match result {
            Ok(report) => {
                *self.last_import.lock().expect("last import") = Some(report.clone());
                // The new items go to the server like any others.
                self.sync.nudge();
                Ok(report)
            }
            Err(kandado_vault::VaultError::Cancelled) => Err(TransferError::Cancelled),
            Err(error) => Err(AppError::Vault(error).into()),
        }
    }

    /// The result of the last import in this run of the app, for the banner
    /// at D-47. Not kept between runs.
    pub fn last_import(&self) -> Option<ImportReport> {
        self.last_import.lock().expect("last import").clone()
    }

    /// Exports the vault to `path`. The account password is asked for again
    /// and checked first, whatever the format: an export holds everything.
    pub async fn export_to(
        self: &Arc<Self>,
        path: &Path,
        format: ExportFormat,
        password: String,
    ) -> Result<ExportSummary, TransferError> {
        let password = zeroize::Zeroizing::new(password);
        self.verify_password(password.to_string())
            .await
            .map_err(|error| match error {
                AppError::WrongPassword => TransferError::WrongPassword,
                other => TransferError::App(other),
            })?;
        let app = self.clone();
        let kdf = self.kdf_params();
        let exported = tokio::task::spawn_blocking(move || {
            app.raw_vault(|store, session| {
                import::export(store, session, format, Some(password.as_str()), kdf, now()).map_err(
                    |error| match error {
                        import::ExportError::Vault(error) => error,
                        import::ExportError::Encode => {
                            kandado_vault::VaultError::Crypto(kandado_crypto::Error::Format)
                        }
                    },
                )
            })
        })
        .await
        .map_err(|_| TransferError::Io("the export stopped".into()))?
        .map_err(|error| TransferError::App(AppError::Vault(error)))?;
        let (items, omitted) = (exported.items, exported.omitted);
        let target = path.to_owned();
        tokio::task::spawn_blocking(move || std::fs::write(&target, exported.bytes.as_slice()))
            .await
            .map_err(|_| TransferError::Io("the export stopped".into()))?
            .map_err(io_error)?;
        Ok(ExportSummary {
            path: path.to_owned(),
            items,
            omitted,
        })
    }

    /// How many items an export of each kind would hold: `(all, csv)`. The
    /// trash is not exported as CSV, nor are cards and identities.
    pub fn export_counts(&self) -> (usize, usize) {
        self.with_session(|session| {
            use kandado_model::ItemKind::{Login, Note, Password};
            let list = session
                .index()
                .list(kandado_vault::index::Filter::default());
            let trashed = session.index().trash_count();
            let csv = list
                .iter()
                .filter(|entry| matches!(entry.kind, Login | Note | Password))
                .count();
            (list.len() + trashed, csv)
        })
    }

    /// Whether to offer the first-run import (D-05): the vault is empty and
    /// the person has not been through it yet.
    pub fn should_offer_import(&self) -> bool {
        !self.settings().import_offered && self.counts() == (0, 0)
    }

    /// The person chose a source, or skipped: the first-run offer is over.
    pub fn import_offer_done(&self) -> Result<(), AppError> {
        self.update_settings(|settings| settings.import_offered = true)
    }
}
