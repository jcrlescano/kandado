//! The local SQLite file. It holds only ciphertext and the metadata needed to
//! unlock: the account's wrapped keys, so unlocking works offline, and the
//! copy of the user key wrapped for this device (Windows Hello).
//!
//! Signing out deletes the file (`Store::wipe`).

use std::path::{Path, PathBuf};

use kandado_crypto::{account::AccountBundle, kdf::KdfParams};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("the local vault could not be read or written: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("the local vault could not be removed: {0}")]
    Io(#[from] std::io::Error),
    #[error("the local vault holds data this version cannot read")]
    Corrupt,
    #[error("that vault or item is not on this computer")]
    NotFound,
    /// A long operation was stopped by the person; nothing was kept.
    #[error("the operation was cancelled")]
    Cancelled,
}

/// What this computer remembers about the signed-in account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAccount {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub bundle: AccountBundle,
}

/// The user key wrapped for this device, so Windows Hello can unlock it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceWrap {
    /// How the wrapping key is protected, such as `hello` or `dpapi`.
    pub method: String,
    pub wrapped: Vec<u8>,
}

/// A vault as stored: its key sealed to the account, its name encrypted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultRow {
    pub id: Uuid,
    pub key_enc: Vec<u8>,
    pub meta_enc: Vec<u8>,
    /// Unix seconds.
    pub created_at: i64,
    pub updated_at: i64,
    /// The server's revision of this vault when we last heard from it, or 0
    /// if the server has never seen it.
    pub server_revision: i64,
    /// Changed here since then: waiting to be sent.
    pub dirty: bool,
}

/// What the server had for an item at the last sync, so a later sync can tell
/// what was changed here and what was changed elsewhere (a three-way merge).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base {
    /// SHA-256 of the item's `content_enc` as the server has it.
    pub digest: Vec<u8>,
    pub vault_id: Uuid,
    pub trashed_at: Option<i64>,
}

/// An item as stored: its key wrapped by the vault key, its content encrypted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRow {
    pub id: Uuid,
    pub vault_id: Uuid,
    pub key_enc: Vec<u8>,
    pub content_enc: Vec<u8>,
    /// Names the saved version of the content; the same on every device.
    pub version_id: Uuid,
    pub created_at: i64,
    pub updated_at: i64,
    /// When it went to the trash, if it is there.
    pub trashed_at: Option<i64>,
    pub server_revision: i64,
    pub dirty: bool,
    pub base: Option<Base>,
}

/// An earlier version of an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionRow {
    /// Identifies this version on this computer, for as long as it is kept.
    pub local_id: i64,
    pub version_id: Uuid,
    pub content_enc: Vec<u8>,
    /// When that version was saved.
    pub saved_at: i64,
}

/// An item deleted here that the server still has: waiting to be sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tombstone {
    pub item_id: Uuid,
    pub vault_id: Uuid,
    pub server_revision: i64,
}

const ITEM_COLUMNS: &str = "id, vault_id, key_enc, content_enc, version_id, created_at, \
     updated_at, trashed_at, server_revision, dirty, base_digest, base_vault_id, base_trashed_at";

fn read_id(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Uuid> {
    let bytes: Vec<u8> = row.get(index)?;
    Uuid::from_slice(&bytes).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Blob,
            Box::new(error),
        )
    })
}

fn read_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<ItemRow> {
    let digest: Option<Vec<u8>> = row.get(10)?;
    let base_vault: Option<Vec<u8>> = row.get(11)?;
    let base = match (digest, base_vault) {
        (Some(digest), Some(_)) => Some(Base {
            digest,
            vault_id: read_id(row, 11)?,
            trashed_at: row.get(12)?,
        }),
        _ => None,
    };
    Ok(ItemRow {
        id: read_id(row, 0)?,
        vault_id: read_id(row, 1)?,
        key_enc: row.get(2)?,
        content_enc: row.get(3)?,
        version_id: read_id(row, 4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        trashed_at: row.get(7)?,
        server_revision: row.get(8)?,
        dirty: row.get::<_, i64>(9)? != 0,
        base,
    })
}

fn read_vault(row: &rusqlite::Row<'_>) -> rusqlite::Result<VaultRow> {
    Ok(VaultRow {
        id: read_id(row, 0)?,
        key_enc: row.get(1)?,
        meta_enc: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
        server_revision: row.get(5)?,
        dirty: row.get::<_, i64>(6)? != 0,
    })
}

fn found(changed: usize) -> Result<(), StoreError> {
    if changed == 0 {
        Err(StoreError::NotFound)
    } else {
        Ok(())
    }
}

pub struct Store {
    path: PathBuf,
    db: Connection,
    /// How many `atomically` calls are open. Only the outermost one starts a
    /// transaction; inner ones join it, so a bigger operation (an import) can
    /// be made of smaller ones.
    depth: std::cell::Cell<u32>,
}

const SCHEMA_VERSION: i64 = 3;

impl Store {
    /// Opens the file, creating it if needed.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let db = Connection::open(path)?;
        Self::init(path.to_owned(), db)
    }

    /// An in-memory store, for tests.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::init(PathBuf::new(), Connection::open_in_memory()?)
    }

    fn init(path: PathBuf, db: Connection) -> Result<Self, StoreError> {
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;
             PRAGMA foreign_keys = ON;",
        )?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if !(0..=SCHEMA_VERSION).contains(&version) {
            return Err(StoreError::Corrupt);
        }
        if version < 1 {
            {
                db.execute_batch(
                    "CREATE TABLE account (
                        id INTEGER PRIMARY KEY CHECK (id = 1),
                        user_id BLOB NOT NULL,
                        device_id BLOB NOT NULL,
                        email TEXT NOT NULL,
                        display_name TEXT NOT NULL,
                        version INTEGER NOT NULL,
                        kdf_salt BLOB NOT NULL,
                        kdf_memory_kib INTEGER NOT NULL,
                        kdf_iterations INTEGER NOT NULL,
                        kdf_parallelism INTEGER NOT NULL,
                        user_key_by_master BLOB NOT NULL,
                        user_key_by_recovery BLOB NOT NULL,
                        public_x25519 BLOB NOT NULL,
                        private_keys_enc BLOB NOT NULL
                    );
                    CREATE TABLE device_wrap (
                        id INTEGER PRIMARY KEY CHECK (id = 1),
                        method TEXT NOT NULL,
                        wrapped BLOB NOT NULL
                    );",
                )?;
                db.pragma_update(None, "user_version", 1)?;
            }
        }
        if version < 2 {
            // Phase 2: vaults, items and their earlier versions. Step 3
            // rebuilds these tables for sync; a file from phase 2 passes
            // through here only if it is older than that.
            db.execute_batch(
                "CREATE TABLE vaults (
                    id BLOB PRIMARY KEY,
                    key_enc BLOB NOT NULL,
                    meta_enc BLOB NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                ) WITHOUT ROWID;
                CREATE TABLE items (
                    id BLOB PRIMARY KEY,
                    vault_id BLOB NOT NULL REFERENCES vaults(id),
                    key_enc BLOB NOT NULL,
                    content_enc BLOB NOT NULL,
                    revision INTEGER NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    trashed_at INTEGER
                ) WITHOUT ROWID;
                CREATE TABLE item_revisions (
                    item_id BLOB NOT NULL REFERENCES items(id) ON DELETE CASCADE,
                    revision INTEGER NOT NULL,
                    content_enc BLOB NOT NULL,
                    saved_at INTEGER NOT NULL,
                    PRIMARY KEY (item_id, revision)
                ) WITHOUT ROWID;",
            )?;
            db.pragma_update(None, "user_version", 2)?;
        }
        if version < 3 {
            // Phase 3: what sync needs. The vault and item tables are rebuilt
            // with the new columns. Rows from phase 2 are kept: each gets a
            // version id and is marked as not yet sent.
            let transaction = db.unchecked_transaction()?;
            // Tables are dropped and renamed in the middle; the foreign keys
            // are checked once, when it is all done.
            transaction.execute_batch(
                "PRAGMA defer_foreign_keys = ON;
                CREATE TABLE vaults_new (
                    id BLOB PRIMARY KEY,
                    key_enc BLOB NOT NULL,
                    meta_enc BLOB NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    server_revision INTEGER NOT NULL DEFAULT 0,
                    dirty INTEGER NOT NULL DEFAULT 1
                ) WITHOUT ROWID;
                INSERT INTO vaults_new (id, key_enc, meta_enc, created_at, updated_at)
                    SELECT id, key_enc, meta_enc, created_at, updated_at FROM vaults;
                CREATE TABLE items_new (
                    id BLOB PRIMARY KEY,
                    vault_id BLOB NOT NULL REFERENCES vaults_new(id),
                    key_enc BLOB NOT NULL,
                    content_enc BLOB NOT NULL,
                    version_id BLOB NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    trashed_at INTEGER,
                    server_revision INTEGER NOT NULL DEFAULT 0,
                    dirty INTEGER NOT NULL DEFAULT 1,
                    base_digest BLOB,
                    base_vault_id BLOB,
                    base_trashed_at INTEGER
                ) WITHOUT ROWID;
                INSERT INTO items_new (id, vault_id, key_enc, content_enc, version_id,
                                       created_at, updated_at, trashed_at)
                    SELECT id, vault_id, key_enc, content_enc, randomblob(16),
                           created_at, updated_at, trashed_at FROM items;
                CREATE TABLE item_revisions_new (
                    local_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    item_id BLOB NOT NULL REFERENCES items_new(id) ON DELETE CASCADE,
                    version_id BLOB NOT NULL,
                    content_enc BLOB NOT NULL,
                    saved_at INTEGER NOT NULL,
                    UNIQUE (item_id, version_id)
                );
                INSERT INTO item_revisions_new (item_id, version_id, content_enc, saved_at)
                    SELECT item_id, randomblob(16), content_enc, saved_at
                    FROM item_revisions ORDER BY item_id, revision;
                DROP TABLE item_revisions;
                DROP TABLE items;
                DROP TABLE vaults;
                ALTER TABLE vaults_new RENAME TO vaults;
                ALTER TABLE items_new RENAME TO items;
                ALTER TABLE item_revisions_new RENAME TO item_revisions;
                CREATE INDEX items_by_vault ON items(vault_id);
                -- Items deleted here that the server still has.
                CREATE TABLE tombstones (
                    item_id BLOB PRIMARY KEY,
                    vault_id BLOB NOT NULL,
                    server_revision INTEGER NOT NULL
                ) WITHOUT ROWID;
                -- How far this computer has read the server's changes.
                CREATE TABLE sync_state (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    cursor INTEGER NOT NULL
                );",
            )?;
            transaction.commit()?;
            db.pragma_update(None, "user_version", 3)?;
        }
        Ok(Self {
            path,
            db,
            depth: std::cell::Cell::new(0),
        })
    }

    // ---- vaults ------------------------------------------------------------

    pub fn vaults(&self) -> Result<Vec<VaultRow>, StoreError> {
        let mut statement = self.db.prepare(
            "SELECT id, key_enc, meta_enc, created_at, updated_at, server_revision, dirty
             FROM vaults ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([], read_vault)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn vault(&self, id: Uuid) -> Result<Option<VaultRow>, StoreError> {
        Ok(self
            .db
            .query_row(
                "SELECT id, key_enc, meta_enc, created_at, updated_at, server_revision, dirty
                 FROM vaults WHERE id = ?1",
                [id.as_bytes().as_slice()],
                read_vault,
            )
            .optional()?)
    }

    /// Saves a vault, replacing the row with the same id.
    pub fn put_vault(&self, vault: &VaultRow) -> Result<(), StoreError> {
        self.db.execute(
            "INSERT OR REPLACE INTO vaults
                (id, key_enc, meta_enc, created_at, updated_at, server_revision, dirty)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                vault.id.as_bytes().as_slice(),
                vault.key_enc,
                vault.meta_enc,
                vault.created_at,
                vault.updated_at,
                vault.server_revision,
                i64::from(vault.dirty)
            ],
        )?;
        Ok(())
    }

    // ---- items -------------------------------------------------------------

    pub fn items(&self) -> Result<Vec<ItemRow>, StoreError> {
        let mut statement = self
            .db
            .prepare(&format!("SELECT {ITEM_COLUMNS} FROM items"))?;
        let rows = statement.query_map([], read_item)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn item(&self, id: Uuid) -> Result<Option<ItemRow>, StoreError> {
        Ok(self
            .db
            .query_row(
                &format!("SELECT {ITEM_COLUMNS} FROM items WHERE id = ?1"),
                [id.as_bytes().as_slice()],
                read_item,
            )
            .optional()?)
    }

    /// Saves an item, replacing the row with the same id.
    pub fn put_item(&self, item: &ItemRow) -> Result<(), StoreError> {
        let (digest, base_vault, base_trashed) = match &item.base {
            Some(base) => (
                Some(base.digest.clone()),
                Some(base.vault_id.as_bytes().to_vec()),
                base.trashed_at,
            ),
            None => (None, None, None),
        };
        // UPDATE or INSERT, not REPLACE: replacing would delete the row first
        // and take its history with it.
        let changed = self.db.execute(
            "UPDATE items SET vault_id = ?2, key_enc = ?3, content_enc = ?4,
                    version_id = ?5, created_at = ?6, updated_at = ?7, trashed_at = ?8,
                    server_revision = ?9, dirty = ?10, base_digest = ?11,
                    base_vault_id = ?12, base_trashed_at = ?13
             WHERE id = ?1",
            params![
                item.id.as_bytes().as_slice(),
                item.vault_id.as_bytes().as_slice(),
                item.key_enc,
                item.content_enc,
                item.version_id.as_bytes().as_slice(),
                item.created_at,
                item.updated_at,
                item.trashed_at,
                item.server_revision,
                i64::from(item.dirty),
                digest,
                base_vault,
                base_trashed
            ],
        )?;
        if changed == 0 {
            self.db.execute(
                &format!(
                    "INSERT INTO items ({ITEM_COLUMNS})
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"
                ),
                params![
                    item.id.as_bytes().as_slice(),
                    item.vault_id.as_bytes().as_slice(),
                    item.key_enc,
                    item.content_enc,
                    item.version_id.as_bytes().as_slice(),
                    item.created_at,
                    item.updated_at,
                    item.trashed_at,
                    item.server_revision,
                    i64::from(item.dirty),
                    digest,
                    base_vault,
                    base_trashed
                ],
            )?;
        }
        Ok(())
    }

    /// Removes an item and its history for good.
    pub fn delete_item(&self, id: Uuid) -> Result<(), StoreError> {
        let changed = self.db.execute(
            "DELETE FROM items WHERE id = ?1",
            [id.as_bytes().as_slice()],
        )?;
        found(changed)
    }

    /// Keeps a version in an item's history. Does nothing if it is there.
    pub fn add_revision(
        &self,
        item: Uuid,
        version_id: Uuid,
        content_enc: &[u8],
        saved_at: i64,
    ) -> Result<(), StoreError> {
        self.db.execute(
            "INSERT OR IGNORE INTO item_revisions (item_id, version_id, content_enc, saved_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                item.as_bytes().as_slice(),
                version_id.as_bytes().as_slice(),
                content_enc,
                saved_at
            ],
        )?;
        Ok(())
    }

    /// Earlier versions of an item, newest first.
    pub fn revisions(&self, item: Uuid) -> Result<Vec<RevisionRow>, StoreError> {
        let mut statement = self.db.prepare(
            "SELECT local_id, version_id, content_enc, saved_at FROM item_revisions
             WHERE item_id = ?1 ORDER BY saved_at DESC, local_id DESC",
        )?;
        let rows = statement.query_map([item.as_bytes().as_slice()], |row| {
            Ok(RevisionRow {
                local_id: row.get(0)?,
                version_id: read_id(row, 1)?,
                content_enc: row.get(2)?,
                saved_at: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Runs several writes as one: all of them happen or none does. Inside
    /// another call it joins that one, and the outermost decides.
    pub fn atomically<T>(
        &self,
        work: impl FnOnce(&Self) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        if self.depth.get() > 0 {
            self.depth.set(self.depth.get() + 1);
            let result = work(self);
            self.depth.set(self.depth.get() - 1);
            return result;
        }
        let transaction = self.db.unchecked_transaction()?;
        self.depth.set(1);
        let result = work(self);
        self.depth.set(0);
        let value = result?;
        transaction.commit()?;
        Ok(value)
    }

    // ---- sync --------------------------------------------------------------

    /// Items deleted here that the server still has.
    pub fn tombstones(&self) -> Result<Vec<Tombstone>, StoreError> {
        let mut statement = self.db.prepare(
            "SELECT item_id, vault_id, server_revision FROM tombstones ORDER BY item_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(Tombstone {
                item_id: read_id(row, 0)?,
                vault_id: read_id(row, 1)?,
                server_revision: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn put_tombstone(&self, tombstone: &Tombstone) -> Result<(), StoreError> {
        self.db.execute(
            "INSERT OR REPLACE INTO tombstones (item_id, vault_id, server_revision)
             VALUES (?1, ?2, ?3)",
            params![
                tombstone.item_id.as_bytes().as_slice(),
                tombstone.vault_id.as_bytes().as_slice(),
                tombstone.server_revision
            ],
        )?;
        Ok(())
    }

    pub fn remove_tombstone(&self, item: Uuid) -> Result<(), StoreError> {
        self.db.execute(
            "DELETE FROM tombstones WHERE item_id = ?1",
            [item.as_bytes().as_slice()],
        )?;
        Ok(())
    }

    /// How far this computer has read the server's changes. 0 at first.
    pub fn cursor(&self) -> Result<i64, StoreError> {
        Ok(self
            .db
            .query_row("SELECT cursor FROM sync_state WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()?
            .unwrap_or(0))
    }

    pub fn set_cursor(&self, cursor: i64) -> Result<(), StoreError> {
        self.db.execute(
            "INSERT OR REPLACE INTO sync_state (id, cursor) VALUES (1, ?1)",
            [cursor],
        )?;
        Ok(())
    }

    /// True once this computer has been told what the server has, or has been
    /// set up to ask: from then on it never makes a first vault on its own.
    pub fn has_synced(&self) -> Result<bool, StoreError> {
        Ok(self.db.query_row(
            "SELECT EXISTS (SELECT 1 FROM sync_state WHERE id = 1)",
            [],
            |row| row.get(0),
        )?)
    }

    /// True if anything here is waiting to be sent.
    pub fn has_unsent(&self) -> Result<bool, StoreError> {
        Ok(self.db.query_row(
            "SELECT EXISTS (SELECT 1 FROM vaults WHERE dirty = 1)
                 OR EXISTS (SELECT 1 FROM items WHERE dirty = 1)
                 OR EXISTS (SELECT 1 FROM tombstones)",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn account(&self) -> Result<Option<LocalAccount>, StoreError> {
        let row = self
            .db
            .query_row(
                "SELECT user_id, device_id, email, display_name, version, kdf_salt,
                        kdf_memory_kib, kdf_iterations, kdf_parallelism,
                        user_key_by_master, user_key_by_recovery, public_x25519, private_keys_enc
                 FROM account WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, u8>(4)?,
                        row.get::<_, Vec<u8>>(5)?,
                        row.get::<_, u32>(6)?,
                        row.get::<_, u32>(7)?,
                        row.get::<_, u32>(8)?,
                        row.get::<_, Vec<u8>>(9)?,
                        row.get::<_, Vec<u8>>(10)?,
                        row.get::<_, Vec<u8>>(11)?,
                        row.get::<_, Vec<u8>>(12)?,
                    ))
                },
            )
            .optional()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let uuid = |bytes: &[u8]| Uuid::from_slice(bytes).map_err(|_| StoreError::Corrupt);
        Ok(Some(LocalAccount {
            user_id: uuid(&row.0)?,
            device_id: uuid(&row.1)?,
            email: row.2,
            display_name: row.3,
            bundle: AccountBundle {
                version: row.4,
                kdf_salt: row.5.try_into().map_err(|_| StoreError::Corrupt)?,
                kdf_params: KdfParams {
                    memory_kib: row.6,
                    iterations: row.7,
                    parallelism: row.8,
                },
                user_key_by_master: row.9,
                user_key_by_recovery: row.10,
                public_x25519: row.11.try_into().map_err(|_| StoreError::Corrupt)?,
                private_keys_enc: row.12,
            },
        }))
    }

    /// Saves the account, replacing any earlier one.
    pub fn save_account(&self, account: &LocalAccount) -> Result<(), StoreError> {
        let bundle = &account.bundle;
        self.db.execute(
            "INSERT OR REPLACE INTO account (
                id, user_id, device_id, email, display_name, version, kdf_salt,
                kdf_memory_kib, kdf_iterations, kdf_parallelism,
                user_key_by_master, user_key_by_recovery, public_x25519, private_keys_enc)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                account.user_id.as_bytes().as_slice(),
                account.device_id.as_bytes().as_slice(),
                account.email,
                account.display_name,
                bundle.version,
                bundle.kdf_salt.as_slice(),
                bundle.kdf_params.memory_kib,
                bundle.kdf_params.iterations,
                bundle.kdf_params.parallelism,
                bundle.user_key_by_master,
                bundle.user_key_by_recovery,
                bundle.public_x25519.as_slice(),
                bundle.private_keys_enc,
            ],
        )?;
        Ok(())
    }

    pub fn device_wrap(&self) -> Result<Option<DeviceWrap>, StoreError> {
        Ok(self
            .db
            .query_row(
                "SELECT method, wrapped FROM device_wrap WHERE id = 1",
                [],
                |row| {
                    Ok(DeviceWrap {
                        method: row.get(0)?,
                        wrapped: row.get(1)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn save_device_wrap(&self, wrap: &DeviceWrap) -> Result<(), StoreError> {
        self.db.execute(
            "INSERT OR REPLACE INTO device_wrap (id, method, wrapped) VALUES (1, ?1, ?2)",
            params![wrap.method, wrap.wrapped],
        )?;
        Ok(())
    }

    pub fn clear_device_wrap(&self) -> Result<(), StoreError> {
        self.db.execute("DELETE FROM device_wrap", [])?;
        Ok(())
    }

    /// Deletes everything this computer holds: the database and its side
    /// files. Used when signing out (D-52).
    pub fn wipe(self) -> Result<(), StoreError> {
        let path = self.path.clone();
        // Closing the connection first releases the file on Windows.
        drop(self.db);
        if path.as_os_str().is_empty() {
            return Ok(());
        }
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let mut name = path.clone().into_os_string();
            name.push(suffix);
            match std::fs::remove_file(PathBuf::from(name)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use kandado_crypto::account;

    use super::*;

    fn sample() -> LocalAccount {
        let user = Uuid::from_u128(7);
        let created = account::create(*user.as_bytes(), "pw", KdfParams::MIN).unwrap();
        LocalAccount {
            user_id: user,
            device_id: Uuid::from_u128(9),
            email: "ana@example.test".into(),
            display_name: "Ana".into(),
            bundle: created.bundle,
        }
    }

    #[test]
    fn the_account_round_trips_and_still_unlocks() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.account().unwrap(), None);
        let saved = sample();
        store.save_account(&saved).unwrap();
        let loaded = store.account().unwrap().unwrap();
        assert_eq!(loaded, saved);
        assert!(
            account::unlock_with_password(*loaded.user_id.as_bytes(), &loaded.bundle, "pw").is_ok()
        );
    }

    #[test]
    fn saving_again_replaces_the_account() {
        let store = Store::open_in_memory().unwrap();
        store.save_account(&sample()).unwrap();
        let mut other = sample();
        other.email = "bob@example.test".into();
        store.save_account(&other).unwrap();
        assert_eq!(store.account().unwrap().unwrap().email, "bob@example.test");
    }

    #[test]
    fn the_device_wrap_can_be_saved_and_cleared() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.device_wrap().unwrap(), None);
        let wrap = DeviceWrap {
            method: "hello".into(),
            wrapped: vec![1, 2, 3],
        };
        store.save_device_wrap(&wrap).unwrap();
        assert_eq!(store.device_wrap().unwrap(), Some(wrap));
        store.clear_device_wrap().unwrap();
        assert_eq!(store.device_wrap().unwrap(), None);
    }

    #[test]
    fn wiping_removes_the_file_and_its_side_files() {
        let dir = std::env::temp_dir().join(format!("kandado-store-{}", std::process::id()));
        let path = dir.join("vault.db");
        let store = Store::open(&path).unwrap();
        store.save_account(&sample()).unwrap();
        assert!(path.exists());
        store.wipe().unwrap();
        assert!(!path.exists());
        assert!(!dir.join("vault.db-wal").exists());
        let reopened = Store::open(&path).unwrap();
        assert_eq!(reopened.account().unwrap(), None);
        reopened.wipe().unwrap();
        let _ = std::fs::remove_dir(&dir);
    }
}
