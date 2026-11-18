//! Bringing items in from other password managers (phase 4).
//!
//! Reading a file is separate from changing the vault. [`read`] turns a file
//! into a [`Plan`]: the items that would be made, and what would be left out
//! and why. The person sees the plan as a preview. Only then does
//! [`apply`] create anything, in one transaction, so a failed import leaves
//! the vault as it was.
//!
//! Nothing is dropped without being said: a field Kandado has no place for
//! becomes a custom field, and an item it cannot use at all is listed in
//! [`Plan::skipped`] with the reason.
//!
//! | Source | Files |
//! | --- | --- |
//! | Proton Pass | ZIP with `data.json`, a PGP-encrypted export, or CSV |
//! | Bitwarden | JSON (not the encrypted one) or CSV |
//! | 1Password | 1PUX |
//! | LastPass | CSV |
//! | Browsers | CSV from Chrome, Edge, Firefox or Safari |
//! | Other | CSV with a header row |
//! | Kandado | Its own JSON or encrypted export |

mod apply;
mod bitwarden;
mod csv_logins;
mod kandado;
mod onepassword;
mod proton;
mod util;

use kandado_model::ItemContent;

pub use apply::{ImportReport, ImportTarget, Progress, apply};
pub use kandado::{ExportError, Exported};

/// Where a file comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    ProtonPass,
    Bitwarden,
    OnePassword,
    LastPass,
    /// Chrome, Edge, Firefox and Safari all export the same kind of CSV.
    Browser,
    /// Any CSV with a header row.
    Csv,
    /// Kandado's own export.
    Kandado,
}

impl Source {
    pub const ALL: [Source; 7] = [
        Self::ProtonPass,
        Self::Bitwarden,
        Self::OnePassword,
        Self::LastPass,
        Self::Browser,
        Self::Csv,
        Self::Kandado,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::ProtonPass => "Proton Pass",
            Self::Bitwarden => "Bitwarden",
            Self::OnePassword => "1Password",
            Self::LastPass => "LastPass",
            Self::Browser => "Chrome, Edge, Firefox, Safari",
            Self::Csv => "Other CSV",
            Self::Kandado => "Kandado",
        }
    }

    /// The files this source accepts, for the picker and the list.
    pub fn files(self) -> &'static str {
        match self {
            Self::ProtonPass => "ZIP, PGP or CSV export",
            Self::Bitwarden => "JSON or CSV",
            Self::OnePassword => "1PUX",
            Self::LastPass => "CSV",
            Self::Browser => "CSV",
            Self::Csv => "",
            Self::Kandado => "JSON or encrypted export",
        }
    }

    /// File name extensions the picker offers.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::ProtonPass => &["zip", "pgp", "csv", "json"],
            Self::Bitwarden => &["json", "csv"],
            Self::OnePassword => &["1pux", "zip"],
            Self::LastPass | Self::Browser | Self::Csv => &["csv"],
            Self::Kandado => &["json", "kandado"],
        }
    }
}

/// Why a file could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    /// The file is not what the source exports, or is damaged.
    #[error("that file is not an export from {0}")]
    NotThisSource(&'static str),
    /// The export is encrypted and needs its password.
    #[error("this export is encrypted and needs its password")]
    NeedsPassphrase,
    #[error("that is not the password for this export")]
    WrongPassphrase,
    /// A kind of export Kandado cannot read, with what to do instead.
    #[error("{0}")]
    Unsupported(&'static str),
}

/// An item that would be made.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedItem {
    pub content: ItemContent,
    /// The vault it was in at the source, if the source has vaults.
    pub vault: Option<String>,
    /// It was in the source's trash.
    pub trashed: bool,
    /// It was an email alias, which Kandado keeps as a note until aliases are built.
    pub from_alias: bool,
}

/// Something that was left out, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// What it was, such as the item's title. Never a secret.
    pub label: String,
    pub reason: String,
}

/// What a file holds, as it would be imported.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub source: Source,
    pub items: Vec<PlannedItem>,
    pub skipped: Vec<Skipped>,
    /// Things the source has that Kandado does not keep, in a sentence each,
    /// such as "3 folders".
    pub ignored: Vec<String>,
    /// How the source's vaults look (name, colour, icon), when it says.
    pub styles: Vec<(String, kandado_model::VaultMeta)>,
}

impl Plan {
    pub(crate) fn new(source: Source) -> Self {
        Self {
            source,
            items: Vec::new(),
            skipped: Vec::new(),
            ignored: Vec::new(),
            styles: Vec::new(),
        }
    }

    /// Items in the file, whether they can be imported or not.
    pub fn found(&self) -> usize {
        self.items.len() + self.skipped.len()
    }

    /// The names of the source's vaults, in order, without repeats.
    pub fn vault_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        for item in &self.items {
            if let Some(name) = item.vault.as_deref()
                && !names.contains(&name)
            {
                names.push(name);
            }
        }
        names
    }

    pub fn trashed(&self) -> usize {
        self.items.iter().filter(|item| item.trashed).count()
    }

    pub fn aliases(&self) -> usize {
        self.items.iter().filter(|item| item.from_alias).count()
    }
}

/// Reads a file into a plan. `passphrase` opens an encrypted export.
pub fn read(source: Source, bytes: &[u8], passphrase: Option<&str>) -> Result<Plan, ImportError> {
    match source {
        Source::ProtonPass => proton::read(bytes, passphrase),
        Source::Bitwarden => bitwarden::read(bytes),
        Source::OnePassword => onepassword::read(bytes),
        Source::LastPass => csv_logins::read(Source::LastPass, bytes),
        Source::Browser => csv_logins::read(Source::Browser, bytes),
        Source::Csv => csv_logins::read(Source::Csv, bytes),
        Source::Kandado => kandado::read(bytes, passphrase),
    }
}

/// Whether a file is an export Kandado made and wants a password.
pub fn is_encrypted_kandado_export(bytes: &[u8]) -> bool {
    kandado::is_encrypted(bytes)
}

pub use kandado::{ExportFormat, export};
