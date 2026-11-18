//! Kandado's own export: three formats, and reading them back.
//!
//! - **JSON**: every vault and item, readable by anyone who has the file.
//! - **CSV**: logins and notes only, in the columns other managers read.
//! - **Encrypted**: the JSON, encrypted with a key made from the account
//!   password. Only Kandado reads it.
//!
//! The encrypted file is: `KDXP`, a version byte (1), the Argon2id settings
//! (memory in KiB, passes, lanes, each four bytes, big-endian), a 16-byte salt,
//! then the encrypted JSON (XChaCha20-Poly1305, context `Purpose::Export`).
//! Tampering with the settings or the salt changes the key, so it fails to open.

use kandado_crypto::{
    Context, Purpose, account, aead,
    context::NO_ID,
    kdf::{self, KdfParams, SALT_LEN},
};
use kandado_model::{ItemContent, ItemData, VaultMeta};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroizing;

use super::{ImportError, Plan, PlannedItem, Source};
use crate::{Session, Store, VaultError, items, vaults};

const NAME: &str = "Kandado";
const MAGIC: &[u8; 4] = b"KDXP";
const VERSION: u8 = 1;
const HEADER: usize = 4 + 1 + 12 + SALT_LEN;
/// Version of the JSON inside, for `kandado_export`.
const JSON_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Encrypted,
    Json,
    Csv,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Encrypted => "kandado",
            Self::Json => "json",
            Self::Csv => "csv",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error(transparent)]
    Vault(#[from] VaultError),
    #[error("the export could not be made")]
    Encode,
}

impl From<kandado_crypto::Error> for ExportError {
    fn from(error: kandado_crypto::Error) -> Self {
        Self::Vault(error.into())
    }
}

/// What an export made.
pub struct Exported {
    /// The file. Plain formats hold every secret: wiped when dropped.
    pub bytes: Zeroizing<Vec<u8>>,
    /// Items in the file.
    pub items: usize,
    /// Items left out because the format cannot hold them (CSV: cards,
    /// identities and the like).
    pub omitted: usize,
}

#[derive(Serialize, Deserialize)]
struct File {
    kandado_export: u8,
    exported_at: i64,
    vaults: Vec<FileVault>,
}

#[derive(Serialize, Deserialize)]
struct FileVault {
    name: String,
    colour: String,
    icon: String,
    items: Vec<FileItem>,
}

#[derive(Serialize, Deserialize)]
struct FileItem {
    trashed: bool,
    item: ItemContent,
}

fn context() -> Context {
    Context::new(Purpose::Export, NO_ID, NO_ID, VERSION)
}

/// Everything in the vault, grouped by vault. Needs the vault unlocked.
fn gather(store: &Store, session: &Session, now: i64) -> Result<File, VaultError> {
    let infos = vaults::list(store, session)?;
    let mut out = File {
        kandado_export: JSON_VERSION,
        exported_at: now,
        vaults: infos
            .iter()
            .map(|info| FileVault {
                name: info.meta.name.clone(),
                colour: info.meta.colour.clone(),
                icon: info.meta.icon.clone(),
                items: Vec::new(),
            })
            .collect(),
    };
    let mut rows = store.items()?;
    rows.sort_by_key(|row| (row.created_at, row.id));
    for row in rows {
        let Some(position) = infos.iter().position(|info| info.id == row.vault_id) else {
            continue;
        };
        // An item this version cannot read is left out of the file, not
        // allowed to stop the export.
        let Ok(item) = items::get(store, session, row.id) else {
            tracing::warn!(item = %row.id, "an item could not be read, so it is not exported");
            continue;
        };
        out.vaults[position].items.push(FileItem {
            trashed: row.trashed_at.is_some(),
            item: item.content,
        });
    }
    Ok(out)
}

fn csv_cell(text: &str) -> &str {
    text
}

fn to_csv(file: &File) -> Result<(Vec<u8>, usize, usize), ExportError> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer
        .write_record([
            "vault", "type", "name", "url", "username", "password", "totp", "note", "favorite",
        ])
        .map_err(|_| ExportError::Encode)?;
    let (mut written, mut omitted) = (0, 0);
    for vault in &file.vaults {
        for entry in vault.items.iter().filter(|entry| !entry.trashed) {
            let content = &entry.item;
            let row: Option<[String; 9]> = match &content.data {
                ItemData::Login(login) => Some([
                    vault.name.clone(),
                    "login".into(),
                    content.title.clone(),
                    login
                        .websites
                        .iter()
                        .map(|site| site.url.as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    login.username.clone(),
                    login.password.expose().to_owned(),
                    login
                        .totp
                        .as_ref()
                        .map(|setup| setup.expose().to_owned())
                        .unwrap_or_default(),
                    content.notes.clone(),
                    u8::from(content.favourite).to_string(),
                ]),
                ItemData::Password(password) => Some([
                    vault.name.clone(),
                    "login".into(),
                    content.title.clone(),
                    String::new(),
                    String::new(),
                    password.password.expose().to_owned(),
                    String::new(),
                    content.notes.clone(),
                    u8::from(content.favourite).to_string(),
                ]),
                ItemData::Note(_) => Some([
                    vault.name.clone(),
                    "note".into(),
                    content.title.clone(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    content.notes.clone(),
                    u8::from(content.favourite).to_string(),
                ]),
                _ => None,
            };
            match row {
                Some(row) => {
                    writer
                        .write_record(row.iter().map(|cell| csv_cell(cell)))
                        .map_err(|_| ExportError::Encode)?;
                    written += 1;
                }
                None => omitted += 1,
            }
        }
    }
    let bytes = writer.into_inner().map_err(|_| ExportError::Encode)?;
    Ok((bytes, written, omitted))
}

/// Makes an export. `password` is required for [`ExportFormat::Encrypted`]:
/// the account password, which the caller has already checked. `params` is the
/// key-derivation cost for the encrypted file.
pub fn export(
    store: &Store,
    session: &Session,
    format: ExportFormat,
    password: Option<&str>,
    params: KdfParams,
    now: i64,
) -> Result<Exported, ExportError> {
    let file = gather(store, session, now)?;
    let count: usize = file.vaults.iter().map(|vault| vault.items.len()).sum();
    match format {
        ExportFormat::Csv => {
            let (bytes, written, omitted) = to_csv(&file)?;
            Ok(Exported {
                bytes: Zeroizing::new(bytes),
                items: written,
                omitted,
            })
        }
        ExportFormat::Json | ExportFormat::Encrypted => {
            let json =
                Zeroizing::new(serde_json::to_vec_pretty(&file).map_err(|_| ExportError::Encode)?);
            if format == ExportFormat::Json {
                return Ok(Exported {
                    bytes: json,
                    items: count,
                    omitted: 0,
                });
            }
            let password = password.ok_or(ExportError::Encode)?;
            let salt = kdf::generate_salt()?;
            let key = account::password_key(password, &salt, &params)?;
            let blob = aead::encrypt(&key, &context(), &json)?;
            let mut bytes = Vec::with_capacity(HEADER + blob.len());
            bytes.extend_from_slice(MAGIC);
            bytes.push(VERSION);
            for value in [params.memory_kib, params.iterations, params.parallelism] {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            bytes.extend_from_slice(&salt);
            bytes.extend_from_slice(&blob);
            Ok(Exported {
                bytes: Zeroizing::new(bytes),
                items: count,
                omitted: 0,
            })
        }
    }
}

pub(super) fn is_encrypted(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

fn open(bytes: &[u8], password: &str) -> Result<Zeroizing<Vec<u8>>, ImportError> {
    if bytes.len() < HEADER || bytes[4] != VERSION {
        return Err(ImportError::NotThisSource(NAME));
    }
    let number = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
    let params = KdfParams {
        memory_kib: number(5),
        iterations: number(9),
        parallelism: number(13),
    };
    // Refuse settings that would make opening a file cost nothing, or a
    // great deal, before any work is done.
    params
        .validate()
        .map_err(|_| ImportError::NotThisSource(NAME))?;
    let salt: [u8; SALT_LEN] = bytes[17..17 + SALT_LEN].try_into().expect("16 bytes");
    let key = account::password_key(password, &salt, &params)
        .map_err(|_| ImportError::NotThisSource(NAME))?;
    let plain = aead::decrypt(&key, &context(), &bytes[HEADER..])
        .map_err(|_| ImportError::WrongPassphrase)?;
    Ok(Zeroizing::new(plain.to_vec()))
}

pub(super) fn read(bytes: &[u8], passphrase: Option<&str>) -> Result<Plan, ImportError> {
    let json: Zeroizing<Vec<u8>> = if is_encrypted(bytes) {
        open(bytes, passphrase.ok_or(ImportError::NeedsPassphrase)?)?
    } else {
        Zeroizing::new(bytes.to_vec())
    };
    // Read as a generic value first: a different JSON file is "not Kandado's".
    let value: Value =
        serde_json::from_slice(&json).map_err(|_| ImportError::NotThisSource(NAME))?;
    if value.get("kandado_export").and_then(Value::as_u64) != Some(u64::from(JSON_VERSION)) {
        return Err(ImportError::NotThisSource(NAME));
    }
    let file: File = serde_json::from_value(value).map_err(|_| ImportError::NotThisSource(NAME))?;
    let mut plan = Plan::new(Source::Kandado);
    for vault in file.vaults {
        plan.styles.push((
            vault.name.clone(),
            VaultMeta::new(vault.name.clone(), &vault.colour, &vault.icon),
        ));
        for entry in vault.items {
            plan.items.push(PlannedItem {
                from_alias: false,
                content: entry.item,
                vault: Some(vault.name.clone()),
                trashed: entry.trashed,
            });
        }
    }
    Ok(plan)
}
