//! The vault end to end, against a real SQLite file: items of every phase 2
//! kind are created, edited, moved, trashed and restored; the file is then
//! read as raw bytes to show that nothing a person typed is in it; and the
//! vault is locked and opened again from the master password alone.

use std::path::PathBuf;

use kandado_crypto::{account, kdf::KdfParams};
use kandado_model::{
    ItemContent, ItemData, ItemKind, VaultMeta,
    item::{Address, Card, CustomField, Expiry, Identity, Login, Password, Website},
};
use kandado_vault::{
    Session, Store, VaultError, history, index::Filter, items, store::LocalAccount, vaults,
};
use uuid::Uuid;

const PASSWORD: &str = "tubig at langis sa kusina";
const DAY: i64 = 86_400;

struct Fixture {
    dir: TempDir,
    store: Store,
    session: Session,
    account: LocalAccount,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("kandado-vault-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(&dir.join("vault.db")).unwrap();
        let user = Uuid::new_v4();
        let created = account::create(*user.as_bytes(), PASSWORD, KdfParams::MIN).unwrap();
        let account = LocalAccount {
            user_id: user,
            device_id: Uuid::new_v4(),
            email: "juan@example.test".into(),
            display_name: "Juan".into(),
            bundle: created.bundle,
        };
        store.save_account(&account).unwrap();
        let mut session = Session::new();
        session.unlock(created.user_key);
        vaults::open(&store, &mut session, &account, 1_000).unwrap();
        Self {
            dir: TempDir(dir),
            store,
            session,
            account,
        }
    }

    /// Locks, closes the file, opens it again and unlocks with the password.
    fn reopen(mut self) -> Self {
        self.session.lock();
        let path = self.dir.0.join("vault.db");
        drop(self.store);
        let store = Store::open(&path).unwrap();
        let account = store.account().unwrap().unwrap();
        let user_key =
            account::unlock_with_password(*account.user_id.as_bytes(), &account.bundle, PASSWORD)
                .unwrap();
        let mut session = Session::new();
        session.unlock(user_key);
        vaults::open(&store, &mut session, &account, 2_000).unwrap();
        Self {
            dir: self.dir,
            store,
            session,
            account,
        }
    }

    fn personal(&self) -> Uuid {
        vaults::list(&self.store, &self.session).unwrap()[0].id
    }
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn login() -> ItemContent {
    ItemContent {
        title: "GitHub-Pamagat".into(),
        notes: "Recovery codes are in the kusina-drawer note.".into(),
        custom_fields: vec![CustomField {
            label: "Security-answer-label".into(),
            value: "Bahay-kubo-sagot".into(),
            hidden: true,
            ..CustomField::default()
        }],
        data: ItemData::Login(Login {
            username: "juan-dc-username".into(),
            password: "Tq7#vL2m!Ox0-r9P".into(),
            websites: vec![Website {
                url: "https://github.example.test".into(),
                ..Website::default()
            }],
            totp: Some("otpauth://totp/GitHub:juan?secret=JBSWY3DPEHPK3PXP".into()),
            ..Login::default()
        }),
        ..ItemContent::default()
    }
}

fn card() -> ItemContent {
    ItemContent::new(
        "BPI-Visa-Pamagat",
        ItemData::Card(Card {
            cardholder: "JUAN-DELA-CRUZ-HOLDER".into(),
            number: "4821009433104821".into(),
            expiry: Some(Expiry {
                month: 8,
                year: 2029,
            }),
            cvv: "214".into(),
            pin: "0912".into(),
            ..Card::default()
        }),
    )
}

fn note() -> ItemContent {
    ItemContent {
        title: "Wi-Fi-sa-bahay".into(),
        notes: "Network: Bahay-5G\nPassword: mangga-tide-42".into(),
        ..ItemContent::default()
    }
}

fn identity() -> ItemContent {
    ItemContent::new(
        "Ako-Identity",
        ItemData::Identity(Box::new(Identity {
            first_name: "Juanito-First".into(),
            last_name: "Cruz-Last".into(),
            email: "juanito@identity.test".into(),
            phone: "+63 917 555 0142".into(),
            address: Address {
                line1: "12 Mabini-Street".into(),
                city: "Quezon-City-Lungsod".into(),
                ..Address::default()
            },
            ..Identity::default()
        })),
    )
}

fn password() -> ItemContent {
    ItemContent::new(
        "Generated-Pamagat",
        ItemData::Password(Password {
            password: "cobalt-Meadow-anchor-7".into(),
            ..Password::default()
        }),
    )
}

#[test]
fn a_new_account_gets_a_personal_vault() {
    let fixture = Fixture::new("personal");
    let vaults = vaults::list(&fixture.store, &fixture.session).unwrap();
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0].meta, VaultMeta::new("Personal", "brass", "vault"));
    assert_eq!(vaults[0].item_count, 0);
    // Opening again does not make a second one.
    let fixture = fixture.reopen();
    assert_eq!(
        vaults::list(&fixture.store, &fixture.session)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn every_kind_is_stored_and_read_back_after_locking() {
    let mut fixture = Fixture::new("kinds");
    let vault = fixture.personal();
    let contents = [login(), card(), note(), identity(), password()];
    let ids: Vec<Uuid> = contents
        .iter()
        .map(|content| {
            items::create(&fixture.store, &mut fixture.session, vault, content, 1_100).unwrap()
        })
        .collect();
    assert_eq!(fixture.session.index().count(Some(vault)), 5);

    let fixture = fixture.reopen();
    for (id, content) in ids.iter().zip(&contents) {
        let item = items::get(&fixture.store, &fixture.session, *id).unwrap();
        assert_eq!(&item.content, content);
        assert_eq!(item.vault_id, vault);
        assert_eq!(item.created_at, 1_100);
    }
    let kinds: Vec<ItemKind> = fixture
        .session
        .index()
        .list(Filter::default())
        .iter()
        .map(|entry| entry.kind)
        .collect();
    assert_eq!(kinds.len(), 5);
    assert_eq!(
        fixture.session.index().search("juanito")[0].title,
        "Ako-Identity"
    );
}

/// The phase's exit criterion: nothing recoverable from the SQLite file
/// without the master password.
#[test]
fn nothing_a_person_typed_is_readable_in_the_file() {
    let mut fixture = Fixture::new("ciphertext");
    let vault = fixture.personal();
    let work = vaults::create(
        &fixture.store,
        &mut fixture.session,
        &VaultMeta::new("Trabaho-Vault-Name", "sky", "bolt"),
        1_050,
    )
    .unwrap();
    for content in [login(), card(), note(), identity(), password()] {
        let id =
            items::create(&fixture.store, &mut fixture.session, vault, &content, 1_100).unwrap();
        // An edit, a move and a trip to the trash leave more rows behind.
        let mut edited = content.clone();
        edited.title = format!("{}-edited", content.title);
        items::update(&fixture.store, &mut fixture.session, id, &edited, 1_200).unwrap();
        items::move_to(&fixture.store, &mut fixture.session, &[id], work, 1_300).unwrap();
    }
    let path = fixture.dir.0.join("vault.db");
    let fixture = fixture.reopen();
    drop(fixture.store);

    let mut bytes = std::fs::read(&path).unwrap();
    for suffix in ["-wal", "-journal"] {
        let mut side = path.clone().into_os_string();
        side.push(suffix);
        bytes.extend(std::fs::read(PathBuf::from(side)).unwrap_or_default());
    }
    assert!(bytes.len() > 4_096, "the file holds the items");
    let text = String::from_utf8_lossy(&bytes).to_lowercase();
    for secret in [
        // Titles, vault names and kinds.
        "pamagat",
        "wi-fi",
        "ako-identity",
        "trabaho",
        "personal",
        "login",
        "identity",
        // Usernames, websites, notes, names, addresses.
        "juan-dc",
        "github",
        "kusina",
        "bahay",
        "mangga",
        "juanito",
        "mabini",
        "quezon",
        "holder",
        "security-answer",
        "sagot",
        "+63",
        // Secrets.
        "tq7#vl2m",
        "jbswy3dp",
        "4821009433104821",
        "cobalt-meadow",
        "otpauth",
        // The master password.
        "tubig",
        "langis",
    ] {
        assert!(
            !text.contains(secret),
            "{secret:?} is readable in the vault file"
        );
    }
    // The only readable text is the account's email and display name, which
    // the server holds too.
    assert!(text.contains("juan@example.test"));
}

#[test]
fn the_wrong_user_key_opens_nothing() {
    let mut fixture = Fixture::new("wrong-key");
    let vault = fixture.personal();
    let id = items::create(&fixture.store, &mut fixture.session, vault, &login(), 1_100).unwrap();
    fixture.session.lock();
    assert!(matches!(
        items::get(&fixture.store, &fixture.session, id),
        Err(VaultError::Locked)
    ));
    assert!(matches!(
        vaults::list(&fixture.store, &fixture.session),
        Err(VaultError::Locked)
    ));

    let mut stranger = Session::new();
    stranger.unlock(kandado_crypto::SecretKey::generate().unwrap());
    assert!(matches!(
        vaults::open(&fixture.store, &mut stranger, &fixture.account, 3_000),
        Err(VaultError::Crypto(_))
    ));
    assert!(stranger.index().is_empty());
}

#[test]
fn edits_keep_history_and_a_version_can_be_restored() {
    let mut fixture = Fixture::new("history");
    let vault = fixture.personal();
    let first = login();
    let id = items::create(&fixture.store, &mut fixture.session, vault, &first, 1_100).unwrap();

    let mut second = first.clone();
    if let ItemData::Login(login) = &mut second.data {
        login.password = "new-password-2".into();
    }
    items::update(&fixture.store, &mut fixture.session, id, &second, 1_200).unwrap();
    // Saving the same content again adds nothing.
    items::update(&fixture.store, &mut fixture.session, id, &second, 1_250).unwrap();
    // Neither does the star.
    items::set_favourite(&fixture.store, &mut fixture.session, id, true, 1_260).unwrap();

    let mut third = second.clone();
    third.favourite = true;
    third.title = "GitHub (personal)".into();
    if let ItemData::Login(login) = &mut third.data {
        login.websites.push(Website {
            url: "https://gist.github.example.test".into(),
            ..Website::default()
        });
    }
    items::update(&fixture.store, &mut fixture.session, id, &third, 1_300).unwrap();

    let versions = history::list(&fixture.store, &fixture.session, id).unwrap();
    let summary: Vec<(i64, bool, &str)> = versions
        .iter()
        .map(|v| (v.saved_at, v.current, v.summary.as_str()))
        .collect();
    assert_eq!(
        summary,
        [
            (1_300, true, "Title changed and 1 more"),
            (1_260, false, "Password changed"),
            (1_100, false, "Created"),
        ]
    );
    // The current version is number 0; the others have numbers of their own.
    let (current, middle, oldest) = (
        versions[0].revision,
        versions[1].revision,
        versions[2].revision,
    );
    assert_eq!(current, history::CURRENT);
    assert!(middle != oldest && middle != 0 && oldest != 0);
    assert_eq!(
        history::get(&fixture.store, &fixture.session, id, oldest).unwrap(),
        first
    );

    // Restoring the oldest saves it as the newest version and keeps the star.
    history::restore(&fixture.store, &mut fixture.session, id, oldest, 1_400).unwrap();
    let item = items::get(&fixture.store, &fixture.session, id).unwrap();
    assert_eq!(item.content.title, first.title);
    assert!(item.content.favourite);
    assert_eq!(
        history::list(&fixture.store, &fixture.session, id)
            .unwrap()
            .len(),
        4
    );
    assert!(matches!(
        history::get(&fixture.store, &fixture.session, id, 99_999),
        Err(VaultError::NotFound)
    ));

    // History survives a lock, and the numbers stay the same.
    let fixture = fixture.reopen();
    assert_eq!(
        history::get(&fixture.store, &fixture.session, id, middle).unwrap(),
        second_with_star(&second)
    );
}

/// The second version as it was kept: the star was added after it was saved.
fn second_with_star(second: &ItemContent) -> ItemContent {
    let mut content = second.clone();
    content.favourite = true;
    content
}

#[test]
fn items_move_between_vaults_and_stay_readable() {
    let mut fixture = Fixture::new("move");
    let personal = fixture.personal();
    let family = vaults::create(
        &fixture.store,
        &mut fixture.session,
        &VaultMeta::new("Family", "coral", "user"),
        1_050,
    )
    .unwrap();
    let a = items::create(
        &fixture.store,
        &mut fixture.session,
        personal,
        &login(),
        1_100,
    )
    .unwrap();
    let b = items::create(
        &fixture.store,
        &mut fixture.session,
        personal,
        &card(),
        1_100,
    )
    .unwrap();
    // History made before the move must still open after it.
    let mut edited = login();
    edited.notes = "edited".into();
    items::update(&fixture.store, &mut fixture.session, a, &edited, 1_150).unwrap();

    items::move_to(&fixture.store, &mut fixture.session, &[a, b], family, 1_200).unwrap();
    let counts: Vec<(String, usize)> = vaults::list(&fixture.store, &fixture.session)
        .unwrap()
        .into_iter()
        .map(|vault| (vault.meta.name, vault.item_count))
        .collect();
    assert_eq!(
        counts,
        [("Personal".to_owned(), 0), ("Family".to_owned(), 2)]
    );

    let fixture = fixture.reopen();
    let item = items::get(&fixture.store, &fixture.session, a).unwrap();
    assert_eq!((item.vault_id, &item.content), (family, &edited));
    let older = history::list(&fixture.store, &fixture.session, a).unwrap()[1].revision;
    assert_eq!(
        history::get(&fixture.store, &fixture.session, a, older).unwrap(),
        login()
    );

    // Moving to a vault that does not exist moves nothing.
    let mut fixture = fixture;
    assert!(matches!(
        items::move_to(
            &fixture.store,
            &mut fixture.session,
            &[a, b],
            Uuid::new_v4(),
            1_300
        ),
        Err(VaultError::NotFound)
    ));
    assert_eq!(fixture.session.index().count(Some(family)), 2);
}

#[test]
fn a_vault_can_be_renamed() {
    let fixture = Fixture::new("rename");
    let id = fixture.personal();
    let meta = VaultMeta::new("Home", "moss", "star");
    vaults::update(&fixture.store, &fixture.session, id, &meta, 1_500).unwrap();
    let fixture = fixture.reopen();
    assert_eq!(
        vaults::list(&fixture.store, &fixture.session).unwrap()[0].meta,
        meta
    );
    assert!(matches!(
        vaults::update(
            &fixture.store,
            &fixture.session,
            Uuid::new_v4(),
            &meta,
            1_600
        ),
        Err(VaultError::NotFound)
    ));
}

#[test]
fn trash_restore_delete_forever_and_empty_trash() {
    let mut fixture = Fixture::new("trash");
    let vault = fixture.personal();
    let mut make = |content: &ItemContent| {
        items::create(&fixture.store, &mut fixture.session, vault, content, 1_100).unwrap()
    };
    let (a, b, c) = (make(&login()), make(&card()), make(&note()));

    // Only trashed items can be deleted forever.
    assert!(matches!(
        items::delete_forever(&fixture.store, &mut fixture.session, &[a]),
        Err(VaultError::NotInTrash)
    ));

    items::trash(&fixture.store, &mut fixture.session, &[a, b, c], 5_000).unwrap();
    assert_eq!(fixture.session.index().count(None), 0);
    assert_eq!(fixture.session.index().trash_count(), 3);
    assert!(fixture.session.index().search("github").is_empty());

    // Undo: back in the vault it came from.
    items::restore(&fixture.store, &mut fixture.session, &[a]).unwrap();
    assert_eq!(fixture.session.index().count(Some(vault)), 1);
    assert_eq!(
        items::get(&fixture.store, &fixture.session, a)
            .unwrap()
            .trashed_at,
        None
    );

    items::delete_forever(&fixture.store, &mut fixture.session, &[b]).unwrap();
    assert!(matches!(
        items::get(&fixture.store, &fixture.session, b),
        Err(VaultError::NotFound)
    ));
    assert_eq!(
        items::empty_trash(&fixture.store, &mut fixture.session).unwrap(),
        1
    );
    assert_eq!(fixture.session.index().trash_count(), 0);

    let fixture = fixture.reopen();
    assert_eq!(fixture.session.index().len(), 1);
    assert!(fixture.session.index().get(c).is_none());
}

#[test]
fn the_trash_empties_itself_after_thirty_days() {
    let mut fixture = Fixture::new("purge");
    let vault = fixture.personal();
    let old = items::create(&fixture.store, &mut fixture.session, vault, &login(), 100).unwrap();
    let recent = items::create(&fixture.store, &mut fixture.session, vault, &card(), 100).unwrap();
    let now = 40 * DAY;
    items::trash(&fixture.store, &mut fixture.session, &[old], now - 30 * DAY).unwrap();
    items::trash(
        &fixture.store,
        &mut fixture.session,
        &[recent],
        now - 3 * DAY,
    )
    .unwrap();

    assert_eq!(items::days_left(now - 3 * DAY, now), 27);
    assert_eq!(items::days_left(now, now), 30);
    assert_eq!(items::days_left(now - 30 * DAY + 1, now), 1);
    assert_eq!(items::days_left(now - 30 * DAY, now), 0);

    assert_eq!(
        items::purge_expired(&fixture.store, &mut fixture.session, now).unwrap(),
        1
    );
    assert!(fixture.session.index().get(old).is_none());
    assert!(fixture.session.index().get(recent).is_some());
}

#[test]
fn duplicates_are_independent_copies() {
    let mut fixture = Fixture::new("duplicate");
    let vault = fixture.personal();
    let mut original = login();
    original.favourite = true;
    let id = items::create(
        &fixture.store,
        &mut fixture.session,
        vault,
        &original,
        1_100,
    )
    .unwrap();
    let copy = items::duplicate(&fixture.store, &mut fixture.session, id, 1_200).unwrap();
    assert_ne!(id, copy);

    let copied = items::get(&fixture.store, &fixture.session, copy).unwrap();
    assert_eq!(copied.content.title, "GitHub-Pamagat (copy)");
    assert!(!copied.content.favourite);
    assert_eq!(copied.content.data, original.data);
    assert_eq!(
        history::list(&fixture.store, &fixture.session, copy)
            .unwrap()
            .len(),
        1
    );

    // Trashing the original leaves the copy alone.
    items::trash(&fixture.store, &mut fixture.session, &[id], 1_300).unwrap();
    items::delete_forever(&fixture.store, &mut fixture.session, &[id]).unwrap();
    assert!(items::get(&fixture.store, &fixture.session, copy).is_ok());
}

/// A blob copied onto another item's row must not decrypt there.
#[test]
fn ciphertext_cannot_be_swapped_between_items() {
    let mut fixture = Fixture::new("swap");
    let vault = fixture.personal();
    let a = items::create(&fixture.store, &mut fixture.session, vault, &login(), 1_100).unwrap();
    let b = items::create(&fixture.store, &mut fixture.session, vault, &card(), 1_100).unwrap();
    let row_a = fixture.store.item(a).unwrap().unwrap();
    // Give item B item A's content: the same key cannot be used, so B fails...
    let mut row_b = fixture.store.item(b).unwrap().unwrap();
    row_b.content_enc = row_a.content_enc.clone();
    fixture.store.put_item(&row_b).unwrap();
    assert!(matches!(
        items::get(&fixture.store, &fixture.session, b),
        Err(VaultError::Crypto(_))
    ));
    // ...and so does B's key put on A's row.
    let row_b = fixture.store.item(b).unwrap().unwrap();
    let mut row_a = row_a;
    row_a.key_enc = row_b.key_enc;
    fixture.store.put_item(&row_a).unwrap();
    assert!(matches!(
        items::get(&fixture.store, &fixture.session, a),
        Err(VaultError::Crypto(_))
    ));
}
