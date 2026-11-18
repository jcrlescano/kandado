//! Sync, without a network: the real client algorithm against an in-memory
//! server that follows the same rules as `kandado-server` (a change is
//! accepted only against the row's current revision, a deletion is a
//! tombstone that beats edits, every version is kept).
//!
//! The last test is the phase's property test: three computers make random
//! edits, some of them offline, sync in random order, and in the end must hold
//! exactly the same vault.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use kandado_crypto::{SecretKey, account, kdf::KdfParams};
use kandado_model::{ItemContent, ItemData, VaultMeta, item::Login};
use kandado_protocol::{
    ChangeResult, ItemVersion, Opaque, PullResponse, PushRequest, PushResponse, SyncItem, SyncVault,
};
use kandado_vault::{
    ApiError, Session, Store, VaultError, history, items,
    store::LocalAccount,
    sync::{self, Access, Remote},
    vaults,
};
use uuid::Uuid;

// ---- the server ---------------------------------------------------------------

#[derive(Default)]
struct State {
    revision: i64,
    vaults: BTreeMap<Uuid, SyncVault>,
    items: BTreeMap<Uuid, SyncItem>,
    versions: HashMap<(Uuid, Uuid), ItemVersion>,
}

struct Server {
    state: Mutex<State>,
}

impl Server {
    fn new() -> Self {
        Self {
            state: Mutex::default(),
        }
    }

    fn versions_of(&self, item: Uuid) -> Vec<Uuid> {
        let state = self.state.lock().unwrap();
        state
            .versions
            .keys()
            .filter(|(id, _)| *id == item)
            .map(|(_, version)| *version)
            .collect()
    }

    fn live_items(&self) -> usize {
        let state = self.state.lock().unwrap();
        state.items.values().filter(|item| !item.deleted).count()
    }
}

/// One computer's connection. Offline, every call fails.
struct Link<'a> {
    server: &'a Server,
    online: &'a AtomicBool,
}

impl Remote for Link<'_> {
    async fn pull(&self, cursor: i64, limit: u32) -> Result<PullResponse, ApiError> {
        if !self.online.load(Ordering::SeqCst) {
            return Err(ApiError::Offline("no network".into()));
        }
        let state = self.server.state.lock().unwrap();
        let mut rows: Vec<(i64, bool, Uuid)> = state
            .vaults
            .values()
            .filter(|v| v.revision > cursor)
            .map(|v| (v.revision, true, v.id))
            .chain(
                state
                    .items
                    .values()
                    .filter(|i| i.revision > cursor)
                    .map(|i| (i.revision, false, i.id)),
            )
            .collect();
        rows.sort();
        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next = rows.last().map_or(cursor, |row| row.0);
        Ok(PullResponse {
            vaults: rows
                .iter()
                .filter(|row| row.1)
                .map(|row| state.vaults[&row.2].clone())
                .collect(),
            items: rows
                .iter()
                .filter(|row| !row.1)
                .map(|row| state.items[&row.2].clone())
                .collect(),
            cursor: next,
            more,
        })
    }

    async fn push(&self, request: &PushRequest) -> Result<PushResponse, ApiError> {
        if !self.online.load(Ordering::SeqCst) {
            return Err(ApiError::Offline("no network".into()));
        }
        let mut state = self.server.state.lock().unwrap();
        let mut vaults = Vec::new();
        for change in &request.vaults {
            vaults.push(match state.vaults.get(&change.id).cloned() {
                None if change.revision == 0 => {
                    state.revision += 1;
                    let mut row = change.clone();
                    row.revision = state.revision;
                    state.vaults.insert(row.id, row.clone());
                    ChangeResult::Applied {
                        revision: row.revision,
                    }
                }
                None => ChangeResult::Rejected,
                Some(current) if current.revision != change.revision => {
                    ChangeResult::Conflict { current }
                }
                Some(mut current) => {
                    state.revision += 1;
                    current.meta_enc = change.meta_enc.clone();
                    current.revision = state.revision;
                    state.vaults.insert(current.id, current.clone());
                    ChangeResult::Applied {
                        revision: current.revision,
                    }
                }
            });
        }
        let mut items = Vec::new();
        for change in &request.items {
            if !state.vaults.contains_key(&change.vault_id) {
                items.push(ChangeResult::Rejected);
                continue;
            }
            let current = state.items.get(&change.id).cloned();
            let result = match current {
                None if change.revision == 0 => {
                    state.revision += 1;
                    let mut row = change.clone();
                    row.revision = state.revision;
                    state.items.insert(row.id, row.clone());
                    if !row.deleted {
                        state.versions.insert(
                            (row.id, row.version_id),
                            ItemVersion {
                                version_id: row.version_id,
                                content_enc: row.content_enc.clone(),
                                key_wrapped: row.key_wrapped.clone(),
                                saved_at: row.updated_at,
                            },
                        );
                    }
                    ChangeResult::Applied {
                        revision: row.revision,
                    }
                }
                None => ChangeResult::Rejected,
                Some(current) if current.deleted || current.revision != change.revision => {
                    ChangeResult::Conflict { current }
                }
                Some(current) => {
                    state.revision += 1;
                    let mut row = change.clone();
                    row.revision = state.revision;
                    if row.deleted {
                        row.content_enc = Opaque::default();
                        row.key_wrapped = Opaque::default();
                        state.versions.retain(|(id, _), _| *id != row.id);
                    } else if row.version_id != current.version_id {
                        state.versions.insert(
                            (row.id, row.version_id),
                            ItemVersion {
                                version_id: row.version_id,
                                content_enc: row.content_enc.clone(),
                                key_wrapped: row.key_wrapped.clone(),
                                saved_at: row.updated_at,
                            },
                        );
                    }
                    state.items.insert(row.id, row.clone());
                    ChangeResult::Applied {
                        revision: row.revision,
                    }
                }
            };
            items.push(result);
        }
        Ok(PushResponse {
            vaults,
            items,
            cursor: state.revision,
        })
    }
}

// ---- a computer ---------------------------------------------------------------

struct Inner {
    store: Store,
    session: Session,
}

struct Computer {
    inner: Mutex<Inner>,
    online: AtomicBool,
    clock: Mutex<i64>,
}

impl Access for Computer {
    fn with<T>(
        &self,
        work: impl FnOnce(&Store, &mut Session) -> Result<T, VaultError>,
    ) -> Result<T, VaultError> {
        let mut inner = self.inner.lock().unwrap();
        let Inner { store, session } = &mut *inner;
        work(store, session)
    }
}

struct World {
    account: LocalAccount,
    user_key: SecretKey,
    server: Server,
}

impl World {
    fn new() -> Self {
        let user = Uuid::from_u128(77);
        let created = account::create(*user.as_bytes(), "pw", KdfParams::MIN).unwrap();
        Self {
            account: LocalAccount {
                user_id: user,
                device_id: Uuid::from_u128(1),
                email: "ana@example.test".into(),
                display_name: "Ana".into(),
                bundle: created.bundle,
            },
            user_key: created.user_key,
            server: Server::new(),
        }
    }

    /// A computer signed in to the account. `first` is the one that makes the
    /// first vault; others wait for it to arrive.
    fn computer(&self, first: bool) -> Computer {
        let store = Store::open_in_memory().unwrap();
        store.save_account(&self.account).unwrap();
        if !first {
            // What signing in to an existing account does.
            store.set_cursor(0).unwrap();
        }
        let mut session = Session::new();
        session.unlock(self.user_key.clone());
        vaults::open(&store, &mut session, &self.account, 1_000).unwrap();
        Computer {
            inner: Mutex::new(Inner { store, session }),
            online: AtomicBool::new(true),
            clock: Mutex::new(2_000),
        }
    }

    async fn sync(&self, computer: &Computer) -> Result<sync::Report, sync::SyncError> {
        let link = Link {
            server: &self.server,
            online: &computer.online,
        };
        sync::run(&link, computer, || {
            let mut clock = computer.clock.lock().unwrap();
            *clock += 1;
            *clock
        })
        .await
    }
}

impl Computer {
    fn now(&self) -> i64 {
        let mut clock = self.clock.lock().unwrap();
        *clock += 1;
        *clock
    }

    fn vault(&self, n: usize) -> Uuid {
        self.with(|store, session| Ok(vaults::list(store, session)?[n].id))
            .unwrap()
    }

    fn create(&self, vault: Uuid, title: &str, password: &str) -> Uuid {
        let now = self.now();
        self.with(|store, session| {
            items::create(store, session, vault, &login(title, password), now)
        })
        .unwrap()
    }

    fn edit(&self, id: Uuid, change: impl FnOnce(&mut ItemContent)) {
        let now = self.now();
        self.with(|store, session| {
            let mut content = items::get(store, session, id)?.content;
            change(&mut content);
            items::update(store, session, id, &content, now)
        })
        .unwrap();
    }

    fn item(&self, id: Uuid) -> Option<items::Item> {
        self.with(|store, session| Ok(items::get(store, session, id).ok()))
            .unwrap()
    }

    fn trash(&self, id: Uuid) {
        let now = self.now();
        self.with(|store, session| items::trash(store, session, &[id], now))
            .unwrap();
    }

    fn history(&self, id: Uuid) -> Vec<history::Version> {
        self.with(|store, session| history::list(store, session, id))
            .unwrap()
    }

    /// Everything this computer holds, decrypted, for comparing computers.
    fn snapshot(&self) -> Snapshot {
        self.with(|store, session| {
            let mut items = BTreeMap::new();
            for row in store.items()? {
                let item = items::get(store, session, row.id)?;
                items.insert(
                    row.id,
                    (
                        item.content.title.clone(),
                        password_of(&item.content),
                        item.content.favourite,
                        item.vault_id,
                        item.trashed_at.is_some(),
                    ),
                );
            }
            let vaults = vaults::list(store, session)?
                .into_iter()
                .map(|vault| (vault.id, vault.meta.name))
                .collect();
            Ok(Snapshot {
                items,
                vaults,
                unsent: store.has_unsent()?,
            })
        })
        .unwrap()
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    items: BTreeMap<Uuid, (String, String, bool, Uuid, bool)>,
    vaults: BTreeMap<Uuid, String>,
    unsent: bool,
}

fn login(title: &str, password: &str) -> ItemContent {
    ItemContent::new(
        title,
        ItemData::Login(Login {
            username: "juan".into(),
            password: password.into(),
            ..Login::default()
        }),
    )
}

fn password_of(content: &ItemContent) -> String {
    match &content.data {
        ItemData::Login(login) => login.password.expose().to_owned(),
        ItemData::Password(password) => password.password.expose().to_owned(),
        _ => String::new(),
    }
}

fn set_password(content: &mut ItemContent, value: &str) {
    if let ItemData::Login(login) = &mut content.data {
        login.password = value.into();
    }
}

// ---- tests ----------------------------------------------------------------------

#[tokio::test]
async fn a_second_computer_gets_what_the_first_made() {
    let world = World::new();
    let (ana, bea) = (world.computer(true), world.computer(false));
    let vault = ana.vault(0);
    let github = ana.create(vault, "GitHub", "pw-one");
    let report = world.sync(&ana).await.unwrap();
    assert_eq!(report.pushed, 2, "the vault and the item");
    assert!(!ana.snapshot().unsent);

    // The second computer has no vault of its own, and gets the first's.
    assert!(bea.snapshot().vaults.is_empty());
    let report = world.sync(&bea).await.unwrap();
    assert!(report.changed && report.pulled == 2);
    let seen = bea.item(github).unwrap();
    assert_eq!(seen.content, login("GitHub", "pw-one"));
    assert_eq!(seen.vault_id, vault);
    assert_eq!(ana.snapshot(), bea.snapshot());
    // Searchable on arrival.
    bea.with(|_, session| {
        assert_eq!(session.index().search("github").len(), 1);
        Ok(())
    })
    .unwrap();

    // An edit travels back, and the old version is in the history there.
    ana.edit(github, |content| set_password(content, "pw-two"));
    world.sync(&ana).await.unwrap();
    world.sync(&bea).await.unwrap();
    assert_eq!(password_of(&bea.item(github).unwrap().content), "pw-two");
    let versions = bea.history(github);
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].summary, "Password changed");
    assert_eq!(ana.snapshot(), bea.snapshot());

    // A sync with nothing new does nothing.
    let report = world.sync(&bea).await.unwrap();
    assert_eq!(
        (
            report.pulled,
            report.pushed,
            report.conflicts,
            report.changed
        ),
        (0, 0, 0, false)
    );
}

#[tokio::test]
async fn work_done_offline_waits_and_is_sent_later() {
    let world = World::new();
    let (ana, bea) = (world.computer(true), world.computer(false));
    world.sync(&ana).await.unwrap();
    world.sync(&bea).await.unwrap();

    ana.online.store(false, Ordering::SeqCst);
    let vault = ana.vault(0);
    let one = ana.create(vault, "One", "pw");
    let two = ana.create(vault, "Two", "pw");
    ana.trash(two);
    assert!(
        matches!(world.sync(&ana).await, Err(sync::SyncError::Api(error)) if error.is_offline())
    );
    assert!(ana.snapshot().unsent, "still waiting");
    assert_eq!(world.server.live_items(), 0);

    ana.online.store(true, Ordering::SeqCst);
    world.sync(&ana).await.unwrap();
    world.sync(&bea).await.unwrap();
    assert!(bea.item(one).is_some());
    assert!(bea.item(two).unwrap().trashed_at.is_some());
    assert_eq!(ana.snapshot(), bea.snapshot());
}

#[tokio::test]
async fn offline_edits_of_different_items_on_both_computers_all_survive() {
    let world = World::new();
    let (ana, bea) = (world.computer(true), world.computer(false));
    let vault = ana.vault(0);
    let (a, b) = (ana.create(vault, "A", "pw"), ana.create(vault, "B", "pw"));
    world.sync(&ana).await.unwrap();
    world.sync(&bea).await.unwrap();

    ana.online.store(false, Ordering::SeqCst);
    bea.online.store(false, Ordering::SeqCst);
    ana.edit(a, |content| set_password(content, "from-ana"));
    let from_ana = ana.create(vault, "New on Ana", "pw");
    bea.edit(b, |content| set_password(content, "from-bea"));
    let from_bea = bea.create(vault, "New on Bea", "pw");
    bea.trash(a);

    ana.online.store(true, Ordering::SeqCst);
    bea.online.store(true, Ordering::SeqCst);
    for computer in [&ana, &bea, &ana, &bea] {
        world.sync(computer).await.unwrap();
    }
    let (left, right) = (ana.snapshot(), bea.snapshot());
    assert_eq!(left, right);
    assert!(!left.unsent);
    assert_eq!(left.items.len(), 4);
    // Ana's edit of A and Bea's trashing of A both stand: each part of the
    // item merged on its own.
    let merged = ana.item(a).unwrap();
    assert_eq!(password_of(&merged.content), "from-ana");
    assert!(merged.trashed_at.is_some());
    assert_eq!(password_of(&ana.item(b).unwrap().content), "from-bea");
    assert!(ana.item(from_ana).is_some() && ana.item(from_bea).is_some());
}

#[tokio::test]
async fn the_same_item_edited_on_both_keeps_the_later_writer_and_loses_nothing() {
    let world = World::new();
    let (ana, bea) = (world.computer(true), world.computer(false));
    let vault = ana.vault(0);
    let id = ana.create(vault, "Bank", "start");
    world.sync(&ana).await.unwrap();
    world.sync(&bea).await.unwrap();

    ana.online.store(false, Ordering::SeqCst);
    bea.online.store(false, Ordering::SeqCst);
    ana.edit(id, |content| set_password(content, "ana-wins-or-not"));
    bea.edit(id, |content| {
        set_password(content, "bea-edit");
        content.notes = "note from bea".into();
    });
    ana.online.store(true, Ordering::SeqCst);
    bea.online.store(true, Ordering::SeqCst);

    // Ana's reaches the server first; Bea's meets it and goes on top.
    world.sync(&ana).await.unwrap();
    let report = world.sync(&bea).await.unwrap();
    assert!(report.conflicts == 0 && report.pushed >= 1, "{report:?}");
    world.sync(&ana).await.unwrap();
    assert_eq!(ana.snapshot(), bea.snapshot());
    let kept = ana.item(id).unwrap();
    assert_eq!(password_of(&kept.content), "bea-edit");
    assert_eq!(kept.content.notes, "note from bea");

    // Every version is still there: the start, Ana's and Bea's.
    assert_eq!(world.server.versions_of(id).len(), 3);
    for computer in [&ana, &bea] {
        let passwords: Vec<String> = computer
            .history(id)
            .iter()
            .map(|version| {
                computer
                    .with(|store, session| {
                        Ok(password_of(&history::get(
                            store,
                            session,
                            id,
                            version.revision,
                        )?))
                    })
                    .unwrap()
            })
            .collect();
        for expected in ["start", "ana-wins-or-not", "bea-edit"] {
            assert!(passwords.iter().any(|p| p == expected), "{passwords:?}");
        }
    }
}

#[tokio::test]
async fn deleting_for_good_beats_an_edit_made_elsewhere() {
    let world = World::new();
    let (ana, bea) = (world.computer(true), world.computer(false));
    let vault = ana.vault(0);
    let id = ana.create(vault, "Old account", "pw");
    world.sync(&ana).await.unwrap();
    world.sync(&bea).await.unwrap();

    ana.online.store(false, Ordering::SeqCst);
    bea.online.store(false, Ordering::SeqCst);
    ana.trash(id);
    ana.with(|store, session| items::delete_forever(store, session, &[id]))
        .unwrap();
    bea.edit(id, |content| set_password(content, "edited too late"));
    ana.online.store(true, Ordering::SeqCst);
    bea.online.store(true, Ordering::SeqCst);

    world.sync(&ana).await.unwrap();
    world.sync(&bea).await.unwrap();
    world.sync(&ana).await.unwrap();
    assert!(ana.item(id).is_none() && bea.item(id).is_none());
    assert_eq!(ana.snapshot(), bea.snapshot());
    assert!(world.server.versions_of(id).is_empty());
    assert!(!ana.snapshot().unsent && !bea.snapshot().unsent);
    // And it is gone from search.
    bea.with(|_, session| {
        assert!(session.index().search("old account").is_empty());
        Ok(())
    })
    .unwrap();
}

#[tokio::test]
async fn a_renamed_vault_and_a_moved_item_follow() {
    let world = World::new();
    let (ana, bea) = (world.computer(true), world.computer(false));
    let personal = ana.vault(0);
    let id = ana.create(personal, "Move me", "pw");
    world.sync(&ana).await.unwrap();
    world.sync(&bea).await.unwrap();

    let work = ana
        .with(|store, session| {
            vaults::create(
                store,
                session,
                &VaultMeta::new("Work", "sky", "bolt"),
                3_000,
            )
        })
        .unwrap();
    ana.with(|store, session| items::move_to(store, session, &[id], work, 3_001))
        .unwrap();
    ana.with(|store, session| {
        vaults::update(
            store,
            session,
            personal,
            &VaultMeta::new("Home", "teal", "user"),
            3_002,
        )
    })
    .unwrap();
    world.sync(&ana).await.unwrap();
    world.sync(&bea).await.unwrap();

    let seen = bea.snapshot();
    assert_eq!(seen, ana.snapshot());
    assert_eq!(seen.items[&id].3, work);
    assert_eq!(seen.vaults[&personal], "Home");
    assert_eq!(seen.vaults[&work], "Work");
    // The moved item still opens on the other computer: its key was wrapped
    // again for the new vault.
    assert_eq!(bea.item(id).unwrap().content, login("Move me", "pw"));
}

#[tokio::test]
async fn history_from_the_server_fills_in_what_this_computer_missed() {
    let world = World::new();
    let (ana, bea) = (world.computer(true), world.computer(false));
    let vault = ana.vault(0);
    let id = ana.create(vault, "Site", "one");
    world.sync(&ana).await.unwrap();
    ana.edit(id, |content| set_password(content, "two"));
    world.sync(&ana).await.unwrap();
    ana.edit(id, |content| set_password(content, "three"));
    world.sync(&ana).await.unwrap();

    // Bea joins late and only has the newest version.
    world.sync(&bea).await.unwrap();
    assert_eq!(bea.history(id).len(), 1);
    let remote: Vec<ItemVersion> = {
        let state = world.server.state.lock().unwrap();
        let mut all: Vec<ItemVersion> = state
            .versions
            .iter()
            .filter(|((item, _), _)| *item == id)
            .map(|(_, version)| version.clone())
            .collect();
        all.sort_by_key(|version| std::cmp::Reverse(version.saved_at));
        all
    };
    assert_eq!(remote.len(), 3);
    let added = bea
        .with(|store, session| history::merge_remote(store, session, id, &remote))
        .unwrap();
    assert_eq!(added, 2);
    assert_eq!(bea.history(id).len(), 3);
    // Merging again adds nothing.
    let again = bea
        .with(|store, session| history::merge_remote(store, session, id, &remote))
        .unwrap();
    assert_eq!(again, 0);
}

#[tokio::test]
async fn a_big_vault_syncs_in_pages_and_batches() {
    let world = World::new();
    let (ana, bea) = (world.computer(true), world.computer(false));
    let vault = ana.vault(0);
    for n in 0..450 {
        ana.create(vault, &format!("Item {n}"), "pw");
    }
    world.sync(&ana).await.unwrap();
    assert_eq!(world.server.live_items(), 450);
    world.sync(&bea).await.unwrap();
    assert_eq!(ana.snapshot(), bea.snapshot());
    assert_eq!(bea.snapshot().items.len(), 450);
}

/// A server that takes nothing: every change is refused.
struct Refusing<'a>(Link<'a>);

impl Remote for Refusing<'_> {
    async fn pull(&self, cursor: i64, limit: u32) -> Result<PullResponse, ApiError> {
        self.0.pull(cursor, limit).await
    }

    async fn push(&self, request: &PushRequest) -> Result<PushResponse, ApiError> {
        Ok(PushResponse {
            vaults: request
                .vaults
                .iter()
                .map(|_| ChangeResult::Applied { revision: 1 })
                .collect(),
            items: request
                .items
                .iter()
                .map(|_| ChangeResult::Rejected)
                .collect(),
            cursor: 0,
        })
    }
}

#[tokio::test]
async fn an_item_the_server_refuses_stays_unsent_and_does_not_loop() {
    let world = World::new();
    let ana = world.computer(true);
    let vault = ana.vault(0);
    let id = ana.create(vault, "Refused", "pw");
    let link = Refusing(Link {
        server: &world.server,
        online: &ana.online,
    });
    let report = sync::run(&link, &ana, || 5_000).await.unwrap();
    assert_eq!(report.rejected, 1);
    assert!(ana.snapshot().unsent, "kept, not dropped");
    assert!(ana.item(id).is_some());
}

/// Opening a file made by phase 2 keeps its vaults, items and history, and
/// sends them on the first sync.
#[tokio::test]
async fn a_phase_2_file_is_upgraded_and_its_data_is_sent() {
    let world = World::new();
    let dir = std::env::temp_dir().join(format!("kandado-sync-upgrade-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("vault.db");

    // Build the phase 2 file by hand, as that version wrote it.
    {
        let store = Store::open(&path).unwrap();
        store.save_account(&world.account).unwrap();
    }
    let computer = {
        let store = Store::open(&path).unwrap();
        let mut session = Session::new();
        session.unlock(world.user_key.clone());
        vaults::open(&store, &mut session, &world.account, 1_000).unwrap();
        Computer {
            inner: Mutex::new(Inner { store, session }),
            online: AtomicBool::new(true),
            clock: Mutex::new(2_000),
        }
    };
    let vault = computer.vault(0);
    let id = computer.create(vault, "Kept", "one");
    computer.edit(id, |content| set_password(content, "two"));
    drop(computer);

    // Downgrade the file to the phase 2 layout, then open it again.
    {
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(
            "DROP TABLE sync_state; DROP TABLE tombstones;
             ALTER TABLE items RENAME TO items_v3; ALTER TABLE item_revisions RENAME TO revs_v3;
             ALTER TABLE vaults RENAME TO vaults_v3;
             CREATE TABLE vaults (id BLOB PRIMARY KEY, key_enc BLOB NOT NULL, meta_enc BLOB NOT NULL,
                created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL) WITHOUT ROWID;
             CREATE TABLE items (id BLOB PRIMARY KEY, vault_id BLOB NOT NULL REFERENCES vaults(id),
                key_enc BLOB NOT NULL, content_enc BLOB NOT NULL, revision INTEGER NOT NULL,
                created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, trashed_at INTEGER) WITHOUT ROWID;
             CREATE TABLE item_revisions (item_id BLOB NOT NULL REFERENCES items(id) ON DELETE CASCADE,
                revision INTEGER NOT NULL, content_enc BLOB NOT NULL, saved_at INTEGER NOT NULL,
                PRIMARY KEY (item_id, revision)) WITHOUT ROWID;
             INSERT INTO vaults SELECT id, key_enc, meta_enc, created_at, updated_at FROM vaults_v3;
             INSERT INTO items SELECT id, vault_id, key_enc, content_enc, 2, created_at, updated_at, trashed_at FROM items_v3;
             INSERT INTO item_revisions SELECT item_id, 1, content_enc, saved_at FROM revs_v3;
             DROP TABLE revs_v3; DROP TABLE items_v3; DROP TABLE vaults_v3;
             PRAGMA user_version = 2;",
        )
        .unwrap();
    }

    let store = Store::open(&path).unwrap();
    let mut session = Session::new();
    session.unlock(world.user_key.clone());
    vaults::open(&store, &mut session, &world.account, 1_000).unwrap();
    assert_eq!(
        items::get(&store, &session, id).unwrap().content,
        login("Kept", "two")
    );
    assert_eq!(history::list(&store, &session, id).unwrap().len(), 2);
    assert!(store.has_unsent().unwrap());
    let computer = Computer {
        inner: Mutex::new(Inner { store, session }),
        online: AtomicBool::new(true),
        clock: Mutex::new(2_000),
    };
    world.sync(&computer).await.unwrap();
    assert!(!computer.snapshot().unsent);
    assert_eq!(world.server.live_items(), 1);
    drop(computer);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the property test -------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

/// One random thing done on one computer.
fn random_step(rng: &mut Rng, computer: &Computer, vault_count: &mut usize, step: usize) {
    let ids: Vec<Uuid> = computer
        .with(|store, _| Ok(store.items()?.into_iter().map(|row| row.id).collect()))
        .unwrap();
    let vaults: Vec<Uuid> = computer
        .with(|store, session| {
            Ok(vaults::list(store, session)?
                .into_iter()
                .map(|v| v.id)
                .collect())
        })
        .unwrap();
    let pick = |rng: &mut Rng, list: &[Uuid]| list[rng.below(list.len())];
    let now = computer.now();
    match rng.below(10) {
        0 | 1 if !vaults.is_empty() => {
            let vault = pick(rng, &vaults);
            computer.create(vault, &format!("item {step}"), &format!("pw {step}"));
        }
        2 | 3 if !ids.is_empty() => {
            let id = pick(rng, &ids);
            let value = format!("edit {step}");
            computer
                .with(|store, session| {
                    if let Ok(item) = items::get(store, session, id) {
                        let mut content = item.content;
                        match rng.below(3) {
                            0 => set_password(&mut content, &value),
                            1 => content.title = format!("title {step}"),
                            _ => content.notes = value,
                        }
                        items::update(store, session, id, &content, now)?;
                    }
                    Ok(())
                })
                .unwrap();
        }
        4 if !ids.is_empty() => {
            let id = pick(rng, &ids);
            let on = rng.below(2) == 0;
            computer
                .with(|store, session| items::set_favourite(store, session, id, on, now))
                .unwrap();
        }
        5 if !ids.is_empty() => {
            let id = pick(rng, &ids);
            computer
                .with(|store, session| items::trash(store, session, &[id], now))
                .unwrap();
        }
        6 if !ids.is_empty() => {
            let id = pick(rng, &ids);
            computer
                .with(|store, session| items::restore(store, session, &[id]))
                .unwrap();
        }
        7 if !ids.is_empty() && !vaults.is_empty() => {
            let (id, vault) = (pick(rng, &ids), pick(rng, &vaults));
            computer
                .with(|store, session| items::move_to(store, session, &[id], vault, now))
                .unwrap();
        }
        8 if !ids.is_empty() => {
            // Deleting for good needs the trash first.
            let id = pick(rng, &ids);
            computer
                .with(|store, session| {
                    items::trash(store, session, &[id], now)?;
                    items::delete_forever(store, session, &[id])
                })
                .unwrap();
        }
        9 if !vaults.is_empty() => {
            if rng.below(3) == 0 && *vault_count < 4 {
                *vault_count += 1;
                computer
                    .with(|store, session| {
                        vaults::create(
                            store,
                            session,
                            &VaultMeta::new(format!("vault {step}"), "sky", "bolt"),
                            now,
                        )
                    })
                    .unwrap();
            } else {
                let vault = pick(rng, &vaults);
                computer
                    .with(|store, session| {
                        vaults::update(
                            store,
                            session,
                            vault,
                            &VaultMeta::new(format!("renamed {step}"), "teal", "user"),
                            now,
                        )
                    })
                    .unwrap();
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn random_concurrent_edits_always_converge() {
    for seed in 1..=150u64 {
        let world = World::new();
        let computers = [
            world.computer(true),
            world.computer(false),
            world.computer(false),
        ];
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut vault_count = 1;
        // The first computer's first vault reaches the server before the
        // others start: a second device only ever joins an existing account.
        world.sync(&computers[0]).await.unwrap();
        world.sync(&computers[1]).await.unwrap();
        world.sync(&computers[2]).await.unwrap();

        for step in 0..70 {
            let computer = &computers[rng.below(3)];
            match rng.below(10) {
                // Going offline and coming back.
                0 => {
                    let now_online = computer.online.load(Ordering::SeqCst);
                    computer.online.store(!now_online, Ordering::SeqCst);
                }
                1..=3 => {
                    let _ = world.sync(computer).await;
                }
                _ => random_step(&mut rng, computer, &mut vault_count, step),
            }
        }

        // Everyone comes back and the server settles.
        for computer in &computers {
            computer.online.store(true, Ordering::SeqCst);
        }
        for _ in 0..6 {
            for computer in &computers {
                world.sync(computer).await.unwrap_or_else(|error| {
                    panic!("seed {seed}: {error}");
                });
            }
        }
        let shots: Vec<Snapshot> = computers.iter().map(Computer::snapshot).collect();
        assert_eq!(shots[0], shots[1], "seed {seed}: computers 0 and 1 differ");
        assert_eq!(shots[1], shots[2], "seed {seed}: computers 1 and 2 differ");
        assert!(!shots[0].unsent, "seed {seed}: something is still unsent");
        // The server holds exactly the live items the computers do.
        assert_eq!(
            world.server.live_items(),
            shots[0].items.len(),
            "seed {seed}: the server and the computers differ"
        );
        // And what they hold is readable and searchable.
        for computer in &computers {
            computer
                .with(|_, session| {
                    assert_eq!(
                        session.index().len(),
                        shots[0].items.len(),
                        "seed {seed}: the index and the items differ"
                    );
                    Ok(())
                })
                .unwrap();
        }
    }
}
