//! Import and export (phase 4): every source, with synthetic export files in
//! `tests/fixtures`, and Kandado's own export read back.
//!
//! The fixtures are written from each manager's documented export layout, not
//! copied from a real export. They check that every field is mapped or
//! reported, and the counts. They cannot show that a manager's real export
//! looks exactly like them; that needs a real file from each (see
//! `docs/implementation-plan/decisions.md`, P4-1).

use std::io::{Cursor, Write};

use kandado_crypto::{account, kdf::KdfParams};
use kandado_model::{ItemContent, ItemData, ItemKind, VaultMeta, item::Login};
use kandado_vault::{
    Session, Store, VaultError,
    import::{
        self, ExportFormat, ImportError, ImportTarget, Plan, Source, apply,
        is_encrypted_kandado_export,
    },
    items,
    store::LocalAccount,
    vaults,
};
use serde_json::Value;
use uuid::Uuid;

const PROTON: &str = include_str!("fixtures/proton-data.json");
const BITWARDEN_JSON: &str = include_str!("fixtures/bitwarden.json");
const BITWARDEN_CSV: &str = include_str!("fixtures/bitwarden.csv");
const LASTPASS: &str = include_str!("fixtures/lastpass.csv");
const CHROME: &str = include_str!("fixtures/chrome.csv");
const FIREFOX: &str = include_str!("fixtures/firefox.csv");
const SAFARI: &str = include_str!("fixtures/safari.csv");
const PROTON_CSV: &str = include_str!("fixtures/proton.csv");
const ONEPASSWORD: &str = include_str!("fixtures/onepassword-export.data");

struct Vault {
    store: Store,
    session: Session,
}

impl Vault {
    fn new() -> Self {
        let user = Uuid::from_u128(5);
        let created = account::create(*user.as_bytes(), "pw", KdfParams::MIN).unwrap();
        let account = LocalAccount {
            user_id: user,
            device_id: Uuid::from_u128(6),
            email: "juan@example.test".into(),
            display_name: "Juan".into(),
            bundle: created.bundle,
        };
        let store = Store::open_in_memory().unwrap();
        store.save_account(&account).unwrap();
        let mut session = Session::new();
        session.unlock(created.user_key);
        vaults::open(&store, &mut session, &account, 1_000).unwrap();
        Self { store, session }
    }

    fn personal(&self) -> Uuid {
        vaults::list(&self.store, &self.session).unwrap()[0].id
    }

    fn import(&mut self, plan: &Plan, target: ImportTarget, trashed: bool) -> import::ImportReport {
        apply(
            &self.store,
            &mut self.session,
            plan,
            target,
            trashed,
            2_000,
            &mut |_| true,
        )
        .unwrap()
    }

    /// Every item, decrypted, with its vault's name and whether it is trashed.
    fn contents(&self) -> Vec<(String, bool, ItemContent)> {
        let names: std::collections::HashMap<Uuid, String> =
            vaults::list(&self.store, &self.session)
                .unwrap()
                .into_iter()
                .map(|vault| (vault.id, vault.meta.name))
                .collect();
        let mut out: Vec<(String, bool, ItemContent)> = self
            .store
            .items()
            .unwrap()
            .into_iter()
            .map(|row| {
                let item = items::get(&self.store, &self.session, row.id).unwrap();
                (
                    names[&row.vault_id].clone(),
                    row.trashed_at.is_some(),
                    item.content,
                )
            })
            .collect();
        out.sort_by(|a, b| (&a.0, &a.2.title).cmp(&(&b.0, &b.2.title)));
        out
    }
}

fn plan(source: Source, text: &str) -> Plan {
    import::read(source, text.as_bytes(), None).unwrap()
}

fn titles(plan: &Plan) -> Vec<&str> {
    plan.items
        .iter()
        .map(|item| item.content.title.as_str())
        .collect()
}

fn find<'a>(plan: &'a Plan, title: &str) -> &'a ItemContent {
    &plan
        .items
        .iter()
        .find(|item| item.content.title == title)
        .unwrap_or_else(|| panic!("no item titled {title}: {:?}", titles(plan)))
        .content
}

fn login(content: &ItemContent) -> &Login {
    match &content.data {
        ItemData::Login(login) => login,
        other => panic!("not a login: {other:?}"),
    }
}

fn field<'a>(content: &'a ItemContent, label: &str) -> Option<(&'a str, bool)> {
    content
        .custom_fields
        .iter()
        .find(|field| field.label == label)
        .map(|field| (field.value.expose(), field.hidden))
}

fn zip_of(name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(name, zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(bytes).unwrap();
    writer.finish().unwrap().into_inner()
}

// ---- Proton Pass -------------------------------------------------------------

#[test]
fn a_proton_zip_with_every_item_type_imports_with_the_counts_of_the_file() {
    let zip = zip_of("Proton Pass/data.json", PROTON.as_bytes());
    let plan = import::read(Source::ProtonPass, &zip, None).unwrap();
    // The file has 9 items. One has a passkey, which is reported, not imported.
    assert_eq!(plan.items.len(), 9);
    assert_eq!(plan.skipped.len(), 1);
    assert!(plan.skipped[0].label.contains("GitHub passkey"));
    assert_eq!(plan.found(), 10);
    assert_eq!(plan.vault_names(), ["Personal", "Work"]);
    assert_eq!((plan.trashed(), plan.aliases()), (1, 1));

    // The kinds: 4 logins (one of them trashed), 3 notes (note, alias, ssh key),
    // 1 card and 1 identity.
    let kinds = |kind: ItemKind| {
        plan.items
            .iter()
            .filter(|i| i.content.kind() == kind)
            .count()
    };
    assert_eq!(kinds(ItemKind::Login), 4);
    assert_eq!(kinds(ItemKind::Note), 3);
    assert_eq!((kinds(ItemKind::Card), kinds(ItemKind::Identity)), (1, 1));

    // A login: username, the email kept as a field, both sites, the code,
    // every extra field, the star from "pinned".
    let github = find(&plan, "GitHub");
    let data = login(github);
    assert_eq!(data.username, "juan-dc");
    assert_eq!(data.password.expose(), "gh-pass-1");
    assert_eq!(data.websites.len(), 2);
    assert!(
        data.totp
            .as_ref()
            .unwrap()
            .expose()
            .starts_with("otpauth://totp")
    );
    assert_eq!(github.notes, "Work account");
    assert!(github.favourite);
    assert_eq!(field(github, "Email"), Some(("juan@example.test", false)));
    assert_eq!(
        field(github, "Recovery email"),
        Some(("r@example.test", false))
    );
    assert_eq!(field(github, "Security answer"), Some(("bahay kubo", true)));
    assert_eq!(
        field(github, "Backup 2FA"),
        Some(("otpauth://totp/x?secret=GEZDGNBVGY3TQOJQ", true))
    );
    // With only an email, the email is the username.
    assert_eq!(login(find(&plan, "Jira")).username, "juan@acme.test");
    // Nothing but a name still makes an item.
    assert_eq!(login(find(&plan, "Bare")).password.expose(), "");

    // The alias is a note holding its address.
    let alias = find(&plan, "Newsletter alias");
    assert!(
        alias
            .notes
            .starts_with("Email alias: quiet.lantern42@passmail.net")
    );
    assert!(alias.notes.contains("for newsletters"));
    // A kind Kandado has no place for is a note with hidden fields.
    let key = find(&plan, "Deploy key");
    assert_eq!(key.kind(), ItemKind::Note);
    assert_eq!(field(key, "Public key").map(|f| f.1), Some(true));
    assert!(field(key, "Private key").unwrap().0.contains("OPENSSH"));

    // A card.
    let card = find(&plan, "BPI Visa");
    let ItemData::Card(data) = &card.data else {
        panic!("not a card")
    };
    assert_eq!(data.number.expose(), "4111111111111111");
    assert_eq!(data.cvv.expose(), "214");
    assert_eq!(data.pin.expose(), "0912");
    assert_eq!(data.expiry.map(|e| (e.month, e.year)), Some((8, 2029)));
    assert_eq!(field(card, "Card type"), Some(("Visa", false)));

    // An identity, with the numbers as documents and the rest as fields.
    let ItemData::Identity(me) = &find(&plan, "Me").data else {
        panic!("not an identity")
    };
    assert_eq!(
        (me.first_name.as_str(), me.last_name.as_str()),
        ("Juan", "Dela Cruz")
    );
    assert_eq!(me.address.line2, "3F");
    assert_eq!(me.address.city, "Quezon City");
    let documents: Vec<&str> = me.documents.iter().map(|d| d.label.as_str()).collect();
    assert_eq!(documents, ["Social security number", "Passport"]);
    let me_content = find(&plan, "Me");
    assert_eq!(field(me_content, "Birthdate"), Some(("1994-02-14", false)));
    assert_eq!(
        field(me_content, "Emergency: Contact"),
        Some(("Maria", false))
    );
}

/// The rule of the phase: no field is dropped without being said. Every text
/// value in the file (apart from ids and the passkey that is reported) is in
/// an imported item.
#[test]
fn every_text_in_a_proton_file_is_in_an_item_or_reported() {
    let plan = plan(Source::ProtonPass, PROTON);
    let everything = serde_json::to_string(
        &plan
            .items
            .iter()
            .map(|item| &item.content)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let root: Value = serde_json::from_str(PROTON).unwrap();
    let mut checked = 0;
    for vault in root["vaults"].as_object().unwrap().values() {
        for item in vault["items"].as_array().unwrap() {
            let mut texts = Vec::new();
            collect(&item["data"]["metadata"]["name"], &mut texts);
            collect(&item["data"]["metadata"]["note"], &mut texts);
            collect(&item["data"]["extraFields"], &mut texts);
            collect(&item["data"]["content"], &mut texts);
            collect(&item["aliasEmail"], &mut texts);
            for text in texts {
                // Escaped as JSON, since that is how the items were written out.
                let escaped = serde_json::to_string(&text).unwrap();
                let escaped = &escaped[1..escaped.len() - 1];
                if item["data"]["content"]["passkeys"]
                    .as_array()
                    .is_some_and(|keys| keys.iter().any(|key| key.to_string().contains(&text)))
                {
                    continue;
                }
                // A card's expiry is kept as a month and a year, not as text.
                if text.len() == 7 && text.as_bytes()[4] == b'-' {
                    continue;
                }
                // Field type names are not content.
                if ["text", "hidden", "totp"].contains(&text.as_str()) {
                    continue;
                }
                checked += 1;
                assert!(everything.contains(escaped), "{text:?} was dropped");
            }
        }
    }
    assert!(checked > 40, "the check looked at {checked} values");
}

fn collect(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(text) if !text.is_empty() => out.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| collect(item, out)),
        Value::Object(map) => map.values().for_each(|item| collect(item, out)),
        _ => {}
    }
}

#[test]
fn a_proton_zip_goes_into_one_vault_or_into_vaults_of_the_same_names() {
    // One chosen vault, nothing in the trash.
    let mut vault = Vault::new();
    let plan = plan(Source::ProtonPass, PROTON);
    let personal = vault.personal();
    let report = vault.import(&plan, ImportTarget::Into(personal), false);
    assert_eq!(
        (
            report.imported,
            report.trash_left_out,
            report.vaults_created
        ),
        (8, 1, 0)
    );
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(vaults::list(&vault.store, &vault.session).unwrap().len(), 1);
    assert!(
        vault
            .contents()
            .iter()
            .all(|(name, trashed, _)| name == "Personal" && !trashed)
    );

    // Vaults of the same names, the trash kept.
    let mut vault = Vault::new();
    let personal = vault.personal();
    let report = vault.import(&plan, ImportTarget::BySource { fallback: personal }, true);
    assert_eq!(
        (
            report.imported,
            report.in_trash,
            report.aliases_as_notes,
            report.vaults_created
        ),
        (9, 1, 1, 1)
    );
    let names: Vec<String> = vaults::list(&vault.store, &vault.session)
        .unwrap()
        .into_iter()
        .map(|vault| vault.meta.name)
        .collect();
    assert_eq!(names, ["Personal", "Work"]);
    let contents = vault.contents();
    assert_eq!(
        contents.iter().filter(|(name, ..)| name == "Work").count(),
        2
    );
    assert_eq!(
        contents.iter().filter(|(_, trashed, _)| *trashed).count(),
        1
    );
    // The imported items are in the search index and waiting to sync.
    assert_eq!(vault.session.index().count(None), 8);
    assert_eq!(vault.session.index().trash_count(), 1);
    assert!(vault.store.has_unsent().unwrap());
}

#[test]
fn a_pgp_encrypted_proton_export_asks_for_its_password_and_gives_the_same_plan() {
    use pgp::{
        composed::{ArmorOptions, MessageBuilder},
        crypto::sym::SymmetricKeyAlgorithm,
        types::StringToKey,
    };

    let encrypt = |data: &[u8], binary: bool| -> Vec<u8> {
        let mut rng = rand::thread_rng();
        let mut message = MessageBuilder::from_bytes("", data.to_vec())
            .seipd_v1(&mut rng, SymmetricKeyAlgorithm::AES256);
        message
            .encrypt_with_password(StringToKey::new_default(&mut rng), &"export pass".into())
            .unwrap();
        if binary {
            message.to_vec(&mut rng).unwrap()
        } else {
            message
                .to_armored_string(&mut rng, ArmorOptions::default())
                .unwrap()
                .into_bytes()
        }
    };
    let expected = plan(Source::ProtonPass, PROTON);
    let zip = zip_of("data.json", PROTON.as_bytes());
    for (name, file) in [
        ("armored json", encrypt(PROTON.as_bytes(), false)),
        ("binary json", encrypt(PROTON.as_bytes(), true)),
        ("armored zip", encrypt(&zip, false)),
    ] {
        assert_eq!(
            import::read(Source::ProtonPass, &file, None).unwrap_err(),
            ImportError::NeedsPassphrase,
            "{name}"
        );
        assert_eq!(
            import::read(Source::ProtonPass, &file, Some("not it")).unwrap_err(),
            ImportError::WrongPassphrase,
            "{name}"
        );
        let opened = import::read(Source::ProtonPass, &file, Some("export pass")).unwrap();
        assert_eq!(opened, expected, "{name}");
    }
}

#[test]
fn a_proton_csv_keeps_its_vaults() {
    let plan = plan(Source::ProtonPass, PROTON_CSV);
    assert_eq!(titles(&plan), ["GitHub", "Wi-Fi at home", "Jira"]);
    assert_eq!(plan.vault_names(), ["Personal", "Work"]);
    let github = find(&plan, "GitHub");
    assert_eq!(login(github).username, "juan-dc");
    assert_eq!(field(github, "Email"), Some(("juan@example.test", false)));
    assert!(login(github).totp.is_some());
    assert_eq!(find(&plan, "Wi-Fi at home").kind(), ItemKind::Note);
    assert_eq!(login(find(&plan, "Jira")).username, "juan@acme.test");
}

// ---- Bitwarden ----------------------------------------------------------------

#[test]
fn bitwarden_json_maps_every_kind_and_reports_what_it_does_not_keep() {
    let plan = plan(Source::Bitwarden, BITWARDEN_JSON);
    assert_eq!(plan.items.len(), 5);
    assert!(plan.skipped.is_empty());
    assert_eq!(plan.trashed(), 1);
    assert!(
        plan.ignored
            .iter()
            .any(|line| line.starts_with("2 folders"))
    );
    assert!(
        plan.ignored
            .iter()
            .any(|line| line.starts_with("1 linked custom field"))
    );

    let github = find(&plan, "GitHub");
    let data = login(github);
    assert!(github.favourite);
    assert_eq!(data.websites.len(), 3);
    assert_eq!(
        data.websites[0].match_mode,
        kandado_model::url_match::MatchMode::Domain
    );
    assert_eq!(
        data.websites[1].match_mode,
        kandado_model::url_match::MatchMode::Host
    );
    assert_eq!(
        data.websites[2].match_mode,
        kandado_model::url_match::MatchMode::Never
    );
    assert!(data.totp.is_some());
    assert_eq!(field(github, "Recovery"), Some(("r@example.test", false)));
    assert_eq!(field(github, "Answer"), Some(("bahay kubo", true)));
    assert_eq!(field(github, "Remember"), Some(("true", false)));
    assert!(field(github, "Linked").is_none());

    let ItemData::Card(card) = &find(&plan, "BPI Visa").data else {
        panic!("not a card")
    };
    assert_eq!(card.expiry.map(|e| (e.month, e.year)), Some((8, 2029)));
    assert_eq!(
        field(find(&plan, "BPI Visa"), "Card type"),
        Some(("Visa", false))
    );
    let me = find(&plan, "Me");
    let ItemData::Identity(identity) = &me.data else {
        panic!("not an identity")
    };
    assert_eq!(identity.address.line2, "3F");
    assert_eq!(identity.documents.len(), 2);
    assert_eq!(field(me, "Title"), Some(("Mr", false)));
    assert_eq!(field(me, "Username"), Some(("juan", false)));
    assert_eq!(find(&plan, "Deploy key").kind(), ItemKind::Note);
    assert!(field(find(&plan, "Deploy key"), "Key fingerprint").is_some());
}

#[test]
fn every_text_in_a_bitwarden_file_is_in_an_item() {
    let plan = plan(Source::Bitwarden, BITWARDEN_JSON);
    let everything = serde_json::to_string(
        &plan
            .items
            .iter()
            .map(|item| &item.content)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let root: Value = serde_json::from_str(BITWARDEN_JSON).unwrap();
    let mut checked = 0;
    for item in root["items"].as_array().unwrap() {
        let mut texts = Vec::new();
        for key in ["name", "notes", "login", "card", "identity", "sshKey"] {
            collect(&item[key], &mut texts);
        }
        for field in item["fields"].as_array().unwrap_or(&Vec::new()) {
            if field["type"] != 3 {
                collect(&field["name"], &mut texts);
                collect(&field["value"], &mut texts);
            }
        }
        for text in texts {
            let escaped = serde_json::to_string(&text).unwrap();
            checked += 1;
            assert!(
                everything.contains(&escaped[1..escaped.len() - 1]),
                "{text:?} was dropped"
            );
        }
    }
    assert!(checked > 30);
}

#[test]
fn an_encrypted_bitwarden_export_says_what_to_do() {
    let error = import::read(
        Source::Bitwarden,
        br#"{"encrypted":true,"data":"2.xx"}"#,
        None,
    )
    .unwrap_err();
    assert!(matches!(error, ImportError::Unsupported(text) if text.contains("Export again")));
}

#[test]
fn bitwarden_csv_reads_custom_fields_sites_and_skips_the_empty_row() {
    let plan = plan(Source::Bitwarden, BITWARDEN_CSV);
    assert_eq!(titles(&plan), ["GitHub", "Wi-Fi", "Netflix"]);
    assert_eq!(plan.skipped.len(), 1, "the row with nothing in it");
    assert!(plan.skipped[0].reason.contains("no title"));
    let github = find(&plan, "GitHub");
    assert_eq!(login(github).websites.len(), 2);
    assert_eq!(field(github, "Answer"), Some(("bahay kubo", false)));
    assert!(github.favourite);
    // A bare base32 key is turned into a setup key Kandado can use.
    assert!(
        login(github)
            .totp
            .as_ref()
            .unwrap()
            .expose()
            .starts_with("otpauth://")
    );
    assert_eq!(find(&plan, "Wi-Fi").kind(), ItemKind::Note);
    assert!(plan.ignored[0].starts_with("2 folders"));
}

// ---- 1Password ------------------------------------------------------------------

#[test]
fn a_1password_export_maps_logins_cards_notes_identities_passwords_and_the_rest() {
    let archive = zip_of("export.data", ONEPASSWORD.as_bytes());
    let plan = import::read(Source::OnePassword, &archive, None).unwrap();
    assert_eq!(plan.items.len(), 6);
    assert_eq!(plan.trashed(), 1);
    // The attached manual is reported, not carried over.
    assert_eq!(plan.skipped.len(), 1);
    assert!(plan.skipped[0].label.contains("Home router attachment"));
    assert_eq!(plan.vault_names(), ["Personal"]);

    let github = find(&plan, "GitHub");
    let data = login(github);
    assert_eq!(
        (data.username.as_str(), data.password.expose()),
        ("juan-dc", "gh-pass-1")
    );
    assert_eq!(
        data.websites.len(),
        2,
        "the address and the one in the list are the same site"
    );
    assert!(data.totp.is_some());
    assert!(github.favourite);
    assert_eq!(
        field(github, "Recovery email"),
        Some(("r@example.test", false))
    );
    assert_eq!(field(github, "Answer"), Some(("bahay kubo", true)));

    let ItemData::Card(card) = &find(&plan, "BPI Visa").data else {
        panic!("not a card")
    };
    assert_eq!(card.number.expose(), "4111111111111111");
    assert_eq!(card.expiry.map(|e| (e.month, e.year)), Some((8, 2029)));
    assert_eq!(
        field(find(&plan, "BPI Visa"), "Card type"),
        Some(("visa", false))
    );

    let me = find(&plan, "Me");
    let ItemData::Identity(identity) = &me.data else {
        panic!("not an identity")
    };
    assert_eq!(identity.first_name, "Juan");
    assert!(identity.address.line1.contains("Quezon City"));
    assert!(field(me, "Identification: birth date").is_some());

    let ItemData::Password(password) = &find(&plan, "Generated").data else {
        panic!("not a password")
    };
    assert_eq!(password.password.expose(), "cobalt-Meadow-anchor-7");
    let router = find(&plan, "Home router");
    assert_eq!(router.kind(), ItemKind::Note);
    assert_eq!(
        field(router, "base station password"),
        Some(("tubig-at-langis", true))
    );
    assert_eq!(
        field(router, "1Password category").map(|f| f.0),
        Some("109")
    );
}

// ---- CSV from other managers and browsers -----------------------------------------

#[test]
fn lastpass_csv_keeps_secure_notes_and_reports_the_empty_row() {
    let plan = plan(Source::LastPass, LASTPASS);
    assert_eq!(titles(&plan), ["GitHub", "Database server", "Netflix"]);
    // The row of only commas is not an item, so it is not counted.
    assert_eq!(plan.skipped.len(), 0);
    assert!(find(&plan, "GitHub").favourite);
    let note = find(&plan, "Database server");
    assert_eq!(note.kind(), ItemKind::Note);
    assert!(note.notes.contains("Hostname:db.example.test"));
    assert!(plan.ignored[0].starts_with("2 folders"));
}

#[test]
fn browser_csv_from_chrome_edge_firefox_and_safari() {
    let chrome = plan(Source::Browser, CHROME);
    assert_eq!(titles(&chrome), ["github.com", "netflix.com"]);
    assert_eq!(
        chrome.skipped.len(),
        0,
        "a row of only commas is not an item"
    );
    assert_eq!(find(&chrome, "github.com").notes, "my note");

    // Firefox has no title: the site is the title.
    let firefox = plan(Source::Browser, FIREFOX);
    assert_eq!(titles(&firefox), ["github.com", "netflix.com"]);
    assert_eq!(
        login(find(&firefox, "netflix.com")).username,
        "family@example.test"
    );

    let safari = plan(Source::Browser, SAFARI);
    assert_eq!(safari.items.len(), 1);
    assert!(login(&safari.items[0].content).totp.is_some());
}

#[test]
fn any_csv_with_a_header_row_works_and_a_wrong_file_is_refused() {
    let csv = "Website,Login,Pass,Comment\nhttps://a.example.test,ana,pw1,hello\n";
    let plan = plan(Source::Csv, csv);
    assert_eq!(plan.items.len(), 1);
    assert_eq!(login(&plan.items[0].content).username, "ana");
    assert_eq!(plan.items[0].content.title, "a.example.test");
    assert_eq!(plan.items[0].content.notes, "hello");

    let not_csv = import::read(Source::Csv, b"just some words\nno columns here", None);
    assert!(matches!(not_csv, Err(ImportError::NotThisSource(_))));
    // The wrong source for a file says so.
    assert!(matches!(
        import::read(Source::OnePassword, BITWARDEN_JSON.as_bytes(), None),
        Err(ImportError::NotThisSource("1Password"))
    ));
    assert!(matches!(
        import::read(Source::ProtonPass, b"PK\x03\x04 not really a zip", None),
        Err(ImportError::NotThisSource("Proton Pass"))
    ));
    assert!(matches!(
        import::read(Source::Bitwarden, b"{\"hello\": 1}", None),
        Err(ImportError::NotThisSource("Bitwarden"))
    ));
}

// ---- applying -------------------------------------------------------------------

#[test]
fn a_failed_import_leaves_the_vault_as_it_was() {
    let mut vault = Vault::new();
    let personal = vault.personal();
    items::create(
        &vault.store,
        &mut vault.session,
        personal,
        &ItemContent::new("Existing", ItemData::default()),
        1_100,
    )
    .unwrap();
    let plan = plan(Source::ProtonPass, PROTON);
    // A vault that does not exist makes the first item fail.
    let result = apply(
        &vault.store,
        &mut vault.session,
        &plan,
        ImportTarget::Into(Uuid::from_u128(99)),
        true,
        2_000,
        &mut |_| true,
    );
    assert!(matches!(result, Err(VaultError::NotFound)));
    assert_eq!(vault.contents().len(), 1);
    assert_eq!(vault.session.index().count(None), 1);

    // Stopped half way, after a second vault was made and 5 items: all of it
    // is undone, and the session agrees with the store.
    let mut vault = Vault::new();
    let personal = vault.personal();
    let mut calls = 0;
    let result = apply(
        &vault.store,
        &mut vault.session,
        &plan,
        ImportTarget::BySource { fallback: personal },
        true,
        2_000,
        &mut |progress| {
            calls += 1;
            assert!(progress.done <= progress.total);
            progress.done < 6
        },
    );
    assert!(matches!(result, Err(VaultError::Cancelled)));
    assert_eq!(calls, 6);
    assert!(vault.contents().is_empty());
    assert_eq!(vaults::list(&vault.store, &vault.session).unwrap().len(), 1);
    assert_eq!(vault.session.index().len(), 0);

    // The same import, run to the end, reports progress after every item.
    let mut calls = 0;
    let report = apply(
        &vault.store,
        &mut vault.session,
        &plan,
        ImportTarget::BySource { fallback: personal },
        true,
        2_000,
        &mut |_| {
            calls += 1;
            true
        },
    )
    .unwrap();
    assert_eq!((calls, report.imported), (9, 9));
    assert_eq!(calls, 9, "progress after each item");
}

// ---- Kandado's own export -----------------------------------------------------------

fn fill(vault: &mut Vault) {
    let personal = vault.personal();
    let work = vaults::create(
        &vault.store,
        &mut vault.session,
        &VaultMeta::new("Work", "sky", "bolt"),
        1_050,
    )
    .unwrap();
    let github = import::read(Source::ProtonPass, PROTON.as_bytes(), None).unwrap();
    for (n, item) in github.items.iter().enumerate() {
        let target = if n % 2 == 0 { personal } else { work };
        let id = items::create(
            &vault.store,
            &mut vault.session,
            target,
            &item.content,
            1_100,
        )
        .unwrap();
        if item.trashed {
            items::trash(&vault.store, &mut vault.session, &[id], 1_200).unwrap();
        }
    }
}

#[test]
fn an_encrypted_export_imports_back_identically() {
    let mut source = Vault::new();
    fill(&mut source);
    let before = source.contents();
    assert_eq!(before.len(), 9);

    let exported = import::export(
        &source.store,
        &source.session,
        ExportFormat::Encrypted,
        Some("the account password"),
        KdfParams::MIN,
        3_000,
    )
    .unwrap();
    assert_eq!((exported.items, exported.omitted), (9, 0));
    assert!(is_encrypted_kandado_export(&exported.bytes));
    // Nothing readable in the file.
    let text = String::from_utf8_lossy(&exported.bytes).to_lowercase();
    for secret in [
        "gh-pass-1",
        "github",
        "4111111111111111",
        "bahay kubo",
        "work",
    ] {
        assert!(!text.contains(secret), "{secret} is readable in the export");
    }

    // The password is asked for, and checked.
    assert_eq!(
        import::read(Source::Kandado, &exported.bytes, None).unwrap_err(),
        ImportError::NeedsPassphrase
    );
    assert_eq!(
        import::read(Source::Kandado, &exported.bytes, Some("not it")).unwrap_err(),
        ImportError::WrongPassphrase
    );
    // A file that was changed does not open.
    let mut tampered = exported.bytes.to_vec();
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    assert_eq!(
        import::read(Source::Kandado, &tampered, Some("the account password")).unwrap_err(),
        ImportError::WrongPassphrase
    );
    let mut other_salt = exported.bytes.to_vec();
    other_salt[20] ^= 1;
    assert!(import::read(Source::Kandado, &other_salt, Some("the account password")).is_err());

    // Into an empty vault elsewhere: the same items, vaults and trash.
    let plan = import::read(
        Source::Kandado,
        &exported.bytes,
        Some("the account password"),
    )
    .unwrap();
    let mut target = Vault::new();
    let fallback = target.personal();
    let report = target.import(&plan, ImportTarget::BySource { fallback }, true);
    assert_eq!((report.imported, report.in_trash), (9, 1));
    assert_eq!(target.contents(), before);
    // The vaults look the same too.
    let looks = |vault: &Vault| -> Vec<VaultMeta> {
        vaults::list(&vault.store, &vault.session)
            .unwrap()
            .into_iter()
            .map(|info| info.meta)
            .collect()
    };
    let mut want = looks(&source);
    let mut got = looks(&target);
    want.sort_by(|a, b| a.name.cmp(&b.name));
    got.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(got, want);
}

#[test]
fn a_json_export_imports_back_identically_and_says_it_is_not_encrypted() {
    let mut source = Vault::new();
    fill(&mut source);
    let before = source.contents();
    let exported = import::export(
        &source.store,
        &source.session,
        ExportFormat::Json,
        None,
        KdfParams::MIN,
        3_000,
    )
    .unwrap();
    assert!(!is_encrypted_kandado_export(&exported.bytes));
    assert!(String::from_utf8_lossy(&exported.bytes).contains("gh-pass-1"));
    let plan = import::read(Source::Kandado, &exported.bytes, None).unwrap();
    let mut target = Vault::new();
    let fallback = target.personal();
    target.import(&plan, ImportTarget::BySource { fallback }, true);
    assert_eq!(target.contents(), before);
    // Another manager's JSON is not Kandado's.
    assert!(matches!(
        import::read(Source::Kandado, BITWARDEN_JSON.as_bytes(), None),
        Err(ImportError::NotThisSource("Kandado"))
    ));
}

#[test]
fn a_csv_export_holds_logins_and_notes_and_says_what_it_left_out() {
    let mut source = Vault::new();
    fill(&mut source);
    let exported = import::export(
        &source.store,
        &source.session,
        ExportFormat::Csv,
        None,
        KdfParams::MIN,
        3_000,
    )
    .unwrap();
    // 9 items: one trashed (left out), a card and an identity (not CSV material).
    assert_eq!(exported.omitted, 2);
    assert_eq!(exported.items, 6);
    let text = String::from_utf8(exported.bytes.to_vec()).unwrap();
    assert!(text.starts_with("vault,type,name,url,username,password,totp,note,favorite"));
    assert!(text.contains("gh-pass-1"));
    assert!(!text.contains("4111111111111111"), "cards are not in CSV");
    assert!(!text.contains("old-pass"), "the trash is not exported");

    // And it reads back as a CSV from another manager would.
    let plan = import::read(Source::Csv, text.as_bytes(), None).unwrap();
    assert_eq!(plan.items.len(), 6);
    let github = find(&plan, "GitHub");
    assert_eq!(login(github).username, "juan-dc");
    assert_eq!(login(github).websites.len(), 2);
    assert!(login(github).totp.is_some());
    assert!(github.favourite);
}

#[test]
fn only_an_unlocked_vault_can_be_exported() {
    let mut source = Vault::new();
    fill(&mut source);
    source.session.lock();
    let result = import::export(
        &source.store,
        &source.session,
        ExportFormat::Json,
        None,
        KdfParams::MIN,
        3_000,
    );
    assert!(result.is_err());
}
