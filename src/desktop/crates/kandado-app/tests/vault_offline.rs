//! The vault through the application service, with no server anywhere: the
//! address points at a closed port. This is phase 2's promise that the app
//! works offline after sign-in: unlock, keep items, copy, lock, come back.

use std::{path::PathBuf, sync::Arc, time::Duration};

use kandado_app::{
    AppError, Filter, KandadoApp, KdfParams, Paths, Phase, UnlockError, VaultError,
    clipboard::{Clipboard, MemoryClipboard},
    device_key::MemoryDeviceKey,
    lock::LockReason,
    secrets::MemorySecrets,
    settings::ClipboardClear,
    testing::seed_account,
};
use kandado_model::{
    ItemContent, ItemData, ItemKind, VaultMeta,
    item::{Card, Login, Password, Website},
    totp::Totp,
};

const PASSWORD: &str = "tubig at langis sa kusina";
const EMAIL: &str = "juan@example.test";

struct Fixture {
    dir: PathBuf,
    app: Arc<KandadoApp>,
    clipboard: Arc<MemoryClipboard>,
}

impl Fixture {
    fn open(dir: PathBuf) -> Self {
        let clipboard = Arc::new(MemoryClipboard::default());
        let app = KandadoApp::with_clipboard(
            Paths {
                data_dir: dir.clone(),
            },
            Arc::new(MemorySecrets::default()),
            Arc::new(MemoryDeviceKey::new()),
            Clipboard::new(clipboard.clone()),
            KdfParams::MIN,
            // Nothing listens here.
            Some("http://127.0.0.1:9"),
        )
        .unwrap();
        Self {
            dir,
            app,
            clipboard,
        }
    }

    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("kandado-app-offline-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        seed_account(
            &Paths {
                data_dir: dir.clone(),
            },
            EMAIL,
            PASSWORD,
        );
        Self::open(dir)
    }

    async fn unlock(&self) {
        assert!(matches!(
            self.app.restore().await.unwrap(),
            Phase::Locked { .. }
        ));
        let phase = self
            .app
            .unlock_with_password(PASSWORD.to_owned())
            .await
            .unwrap();
        assert_eq!(
            phase,
            Phase::Unlocked {
                email: EMAIL.to_owned()
            }
        );
    }
}

fn login() -> ItemContent {
    ItemContent::new(
        "GitHub",
        ItemData::Login(Login {
            username: "juan-dc".into(),
            password: "Tq7#vL2m!Ox0-r9P".into(),
            websites: vec![Website {
                url: "https://github.com".into(),
                ..Website::default()
            }],
            totp: Some("otpauth://totp/GitHub:juan?secret=JBSWY3DPEHPK3PXP".into()),
            ..Login::default()
        }),
    )
}

#[tokio::test]
async fn unlock_offline_keep_items_lock_and_come_back() {
    let fixture = Fixture::new("round-trip");
    // Locked: nothing to see.
    assert!(fixture.app.entries(Filter::default()).is_empty());
    assert!(matches!(
        fixture.app.vaults(),
        Err(AppError::Vault(VaultError::Locked))
    ));

    fixture.unlock().await;
    let vaults = fixture.app.vaults().unwrap();
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0].meta.name, "Personal");

    let personal = vaults[0].id;
    let id = fixture.app.create_item(personal, &login()).unwrap();
    fixture
        .app
        .create_item(
            personal,
            &ItemContent::new(
                "BPI Visa",
                ItemData::Card(Card {
                    number: "4821009433104821".into(),
                    ..Card::default()
                }),
            ),
        )
        .unwrap();
    assert_eq!(fixture.app.counts(), (2, 0));
    assert_eq!(fixture.app.search("git")[0].id, id);
    assert_eq!(
        fixture.app.entries(Filter {
            kind: Some(ItemKind::Card),
            ..Filter::default()
        })[0]
            .subtitle,
        "•••• 4821"
    );

    // Lock: the lists empty and nothing can be read.
    fixture.app.lock(LockReason::Manual);
    assert!(fixture.app.entries(Filter::default()).is_empty());
    assert!(fixture.app.search("git").is_empty());
    assert!(fixture.app.item_detail(id).is_err());
    assert!(fixture.app.reveal(id, None, 1).is_err());

    // A wrong password opens nothing.
    assert!(matches!(
        fixture.app.unlock_with_password("not it".into()).await,
        Err(UnlockError::WrongPassword { tries_left: 4 })
    ));
    assert!(fixture.app.entries(Filter::default()).is_empty());

    // Close the app entirely and start it again.
    let dir = fixture.dir.clone();
    drop(fixture);
    let fixture = Fixture::open(dir);
    fixture.unlock().await;
    assert_eq!(fixture.app.counts(), (2, 0));
    assert_eq!(
        fixture.app.reveal(id, None, 1).unwrap().as_str(),
        "Tq7#vL2m!Ox0-r9P"
    );
}

#[tokio::test]
async fn copies_clear_and_locking_clears_at_once() {
    let fixture = Fixture::new("copy");
    fixture.unlock().await;
    let vault = fixture.app.vaults().unwrap()[0].id;
    let id = fixture.app.create_item(vault, &login()).unwrap();
    let detail = fixture.app.item_detail(id).unwrap();

    // A username is not a secret: it stays on the clipboard.
    let copied = fixture
        .app
        .copy_field(id, detail.username_field.unwrap())
        .unwrap();
    assert_eq!(
        (copied.label.as_str(), copied.clears_in),
        ("Username", None)
    );
    assert_eq!(fixture.clipboard.text().as_deref(), Some("juan-dc"));

    // A password says when it clears.
    let copied = fixture
        .app
        .copy_field(id, detail.password_field.unwrap())
        .unwrap();
    assert_eq!(
        (copied.label.as_str(), copied.clears_in),
        ("Password", Some(60))
    );
    assert_eq!(
        fixture.clipboard.text().as_deref(),
        Some("Tq7#vL2m!Ox0-r9P")
    );

    // The setting changes the time.
    fixture
        .app
        .update_settings(|settings| settings.clipboard_clear = ClipboardClear::Seconds30)
        .unwrap();
    let copied = fixture
        .app
        .copy_field(id, detail.totp_field.unwrap())
        .unwrap();
    assert_eq!(copied.clears_in, Some(30));
    // The code is copied, never the setup key.
    let code = fixture.clipboard.text().unwrap();
    assert!(
        code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()),
        "{code}"
    );
    let live = fixture.app.totp_code(id).unwrap().unwrap();
    let expected = Totp::parse("JBSWY3DPEHPK3PXP").unwrap();
    assert_eq!(live.period, 30);
    assert!(
        // The 30 s window may roll over between the two calls.
        code == live.digits
            || code
                == expected
                    .code_at(kandado_app::vault::now() as u64 - 30)
                    .digits
    );

    // Locking clears the clipboard without waiting.
    fixture.app.lock(LockReason::Manual);
    assert_eq!(fixture.clipboard.text(), None);
}

#[tokio::test]
async fn a_copied_secret_clears_after_its_time() {
    // The shortest setting is 30 s, so this uses the clipboard directly with
    // the app's backend to keep the test fast; the service passes the delay.
    let backend = Arc::new(MemoryClipboard::default());
    let clipboard = Clipboard::new(backend.clone());
    clipboard
        .copy("Tq7#vL2m", Some(Duration::from_millis(50)))
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(backend.text(), None);
}

#[tokio::test]
async fn edit_history_trash_and_undo_through_the_service() {
    let fixture = Fixture::new("edit");
    fixture.unlock().await;
    let personal = fixture.app.vaults().unwrap()[0].id;
    let work = fixture
        .app
        .create_vault(&VaultMeta::new("Work", "sky", "bolt"))
        .unwrap();
    let id = fixture.app.create_item(personal, &login()).unwrap();

    // Saving with another vault chosen moves the item.
    let mut edited = login();
    if let ItemData::Login(login) = &mut edited.data {
        login.password = "second-password".into();
    }
    fixture.app.update_item(id, work, &edited).unwrap();
    assert_eq!(fixture.app.item(id).unwrap().vault_id, work);
    let counts: Vec<usize> = fixture
        .app
        .vaults()
        .unwrap()
        .iter()
        .map(|vault| vault.item_count)
        .collect();
    assert_eq!(counts, [0, 1]);

    // History: the old password is in version 1, concealed until revealed.
    let versions = fixture.app.history(id).unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].summary, "Password changed");
    let old = fixture.app.version_detail(id, 1).unwrap();
    assert!(!format!("{old:?}").contains("Tq7#vL2m"));
    assert_eq!(
        fixture.app.reveal(id, Some(1), 1).unwrap().as_str(),
        "Tq7#vL2m!Ox0-r9P"
    );
    fixture.app.restore_version(id, 1).unwrap();
    assert_eq!(
        fixture.app.reveal(id, None, 1).unwrap().as_str(),
        "Tq7#vL2m!Ox0-r9P"
    );

    // Favourite, duplicate.
    fixture.app.set_favourite(&[id], true).unwrap();
    assert!(fixture.app.entry(id).unwrap().favourite);
    let copy = fixture.app.duplicate_item(id).unwrap();
    assert_eq!(fixture.app.entry(copy).unwrap().title, "GitHub (copy)");

    // Trash with undo, then for good.
    fixture.app.trash_items(&[id, copy]).unwrap();
    assert_eq!(fixture.app.counts(), (0, 2));
    assert_eq!(fixture.app.item_detail(id).unwrap().days_left, Some(30));
    fixture.app.restore_items(&[id]).unwrap();
    assert_eq!(fixture.app.counts(), (1, 1));
    assert!(matches!(
        fixture.app.delete_forever(&[id]),
        Err(AppError::Vault(VaultError::NotInTrash))
    ));
    assert_eq!(fixture.app.empty_trash().unwrap(), 1);
    assert_eq!(fixture.app.counts(), (1, 0));

    // A generated password saved on its own.
    let generated = fixture
        .app
        .create_item(
            personal,
            &ItemContent::new(
                "Generated password",
                ItemData::Password(Password {
                    password: "cobalt-Meadow-anchor-7".into(),
                    ..Password::default()
                }),
            ),
        )
        .unwrap();
    let detail = fixture.app.item_detail(generated).unwrap();
    assert_eq!(detail.password_field, Some(0));
    assert_eq!(detail.strength.unwrap().label(), "Strong");
}

#[tokio::test]
async fn deleting_a_vault_through_the_service() {
    let fixture = Fixture::new("delete-vault");
    fixture.unlock().await;
    let personal = fixture.app.vaults().unwrap()[0].id;
    let kept = fixture.app.create_item(personal, &login()).unwrap();

    // The only vault stays.
    assert!(matches!(
        fixture.app.delete_vault(personal),
        Err(AppError::Vault(VaultError::LastVault))
    ));

    let work = fixture
        .app
        .create_vault(&VaultMeta::new("Work", "sky", "bolt"))
        .unwrap();
    let jira = fixture.app.create_item(work, &login()).unwrap();
    let old = fixture.app.create_item(work, &login()).unwrap();
    fixture.app.trash_items(&[old]).unwrap();
    assert_eq!(fixture.app.vaults().unwrap()[1].item_count, 1);
    assert_eq!(fixture.app.counts(), (2, 1));

    fixture.app.delete_vault(work).unwrap();
    let vaults = fixture.app.vaults().unwrap();
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0].id, personal);
    // Its items went with it, the trashed one too; the other vault's stayed.
    assert_eq!(fixture.app.counts(), (1, 0));
    assert!(fixture.app.entry(jira).is_none() && fixture.app.entry(old).is_none());
    assert!(fixture.app.entry(kept).is_some());
    assert!(
        fixture
            .app
            .entries(Filter {
                vault: Some(work),
                ..Filter::default()
            })
            .is_empty()
    );
    assert!(matches!(
        fixture.app.delete_vault(work),
        Err(AppError::Vault(VaultError::NotFound))
    ));
    // Now the last one again.
    assert!(matches!(
        fixture.app.delete_vault(personal),
        Err(AppError::Vault(VaultError::LastVault))
    ));

    // It stays deleted after the app starts again.
    let dir = fixture.dir.clone();
    drop(fixture);
    let fixture = Fixture::open(dir);
    fixture.unlock().await;
    let vaults = fixture.app.vaults().unwrap();
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0].id, personal);
    assert_eq!(fixture.app.counts(), (1, 0));
}
