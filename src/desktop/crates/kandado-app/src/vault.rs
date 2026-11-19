//! Vaults and items as the host sees them. Lists and detail panes get view
//! models: titles, labels and plain values. A secret leaves this module only
//! at the moment a person reveals or copies it ([`KandadoApp::reveal`],
//! [`KandadoApp::copy_field`]), and forms get the full content only while
//! they are open ([`KandadoApp::item`]).
//!
//! Everything here is local: SQLite and the keys in memory. Nothing waits on
//! the network, so these methods are called straight from the views.

use std::time::{SystemTime, UNIX_EPOCH};

use kandado_model::{
    ItemContent, ItemData, ItemKind, VaultMeta,
    strength::{self, Strength},
    totp::{Code, Totp},
};
use kandado_vault::{
    Session, Store, VaultError, history,
    index::{Entry, Filter},
    items::{self, Item},
    vaults::{self, VaultInfo},
};
use uuid::Uuid;
use zeroize::Zeroizing;

pub use kandado_vault::history::Version;

use crate::service::{AppError, KandadoApp};

/// The current time as Unix seconds.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// Whole days until a trashed item is deleted for good.
pub fn days_left(trashed_at: i64) -> i64 {
    items::days_left(trashed_at, now())
}

/// The glyph beside a field. The host maps these to its icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldIcon {
    User,
    Key,
    Globe,
    Card,
    Clock,
    Shield,
    Mail,
    Eye,
    Id,
    Note,
    None,
}

/// One labelled value on the detail pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// Identifies the field to [`KandadoApp::reveal`] and [`KandadoApp::copy_field`].
    pub index: usize,
    pub label: String,
    pub icon: FieldIcon,
    /// The text to show. `None` for a concealed field: ask to reveal it.
    pub value: Option<String>,
    pub mono: bool,
    /// A website address.
    pub link: bool,
    /// The two-factor code: show the live code from [`KandadoApp::totp_code`].
    pub totp: bool,
}

impl Field {
    pub fn concealed(&self) -> bool {
        self.value.is_none() && !self.totp
    }
}

/// Fields that belong together, with an optional heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub label: Option<&'static str>,
    pub fields: Vec<Field>,
}

/// What the detail pane shows for one item. Holds no secret.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemDetail {
    pub id: Uuid,
    pub vault_id: Uuid,
    pub kind: ItemKind,
    pub title: String,
    pub favourite: bool,
    pub sections: Vec<Section>,
    /// The note text. Notes are not concealed.
    pub note: String,
    /// How strong the password is, for a login or a password item.
    pub strength: Option<Strength>,
    pub username_field: Option<usize>,
    pub password_field: Option<usize>,
    pub totp_field: Option<usize>,
    /// Unix seconds.
    pub created_at: i64,
    pub updated_at: i64,
    /// Days until it is deleted for good, when it is in the trash.
    pub days_left: Option<i64>,
    /// For a passkey (D-13): the site it signs in to.
    pub passkey_site: Option<String>,
}

/// A passkey's credential id as D-13 shows it: the first eight bytes in hex,
/// in groups of four, such as `7F3A·91C0·E2B4·C21E`. Enough to tell two apart
/// and to match the list a site shows; not the whole id.
pub fn key_id(credential_id: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    match URL_SAFE_NO_PAD.decode(credential_id.trim_end_matches('=')) {
        Ok(bytes) if !bytes.is_empty() => bytes
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|pair| pair.concat())
            .collect::<Vec<_>>()
            .join("\u{b7}"),
        _ => credential_id.chars().take(16).collect(),
    }
}

/// What a copy did, for the toast: "Password copied. Clears in 60 s."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    pub label: String,
    pub clears_in: Option<u64>,
}

/// A field with its value, before the secret ones are held back.
struct Raw {
    label: String,
    icon: FieldIcon,
    /// What is shown when revealed.
    display: Zeroizing<String>,
    /// What is copied, when that differs from what is shown.
    copy: Option<Zeroizing<String>>,
    concealed: bool,
    mono: bool,
    link: bool,
    totp: bool,
}

impl Raw {
    fn plain(label: &str, icon: FieldIcon, value: &str) -> Self {
        Self {
            label: label.to_owned(),
            icon,
            display: Zeroizing::new(value.to_owned()),
            copy: None,
            concealed: false,
            mono: false,
            link: false,
            totp: false,
        }
    }

    fn secret(label: &str, icon: FieldIcon, value: &str) -> Self {
        Self {
            concealed: true,
            ..Self::plain(label, icon, value)
        }
    }

    fn mono(mut self) -> Self {
        self.mono = true;
        self
    }
}

/// "4821 0094 3310 4821": card numbers read in groups of four.
fn grouped_card_number(number: &str) -> Zeroizing<String> {
    let digits: Zeroizing<String> =
        Zeroizing::new(number.chars().filter(|c| !c.is_whitespace()).collect());
    let mut grouped = Zeroizing::new(String::with_capacity(digits.len() + digits.len() / 4));
    for (position, digit) in digits.chars().enumerate() {
        if position > 0 && position % 4 == 0 {
            grouped.push(' ');
        }
        grouped.push(digit);
    }
    grouped
}

fn join(parts: &[&str], separator: &str) -> String {
    parts
        .iter()
        .filter(|part| !part.trim().is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(separator)
}

/// Every field of an item, in display order, grouped. Empty values are left out.
fn raw_sections(content: &ItemContent) -> Vec<(Option<&'static str>, Vec<Raw>)> {
    let mut sections: Vec<(Option<&'static str>, Vec<Raw>)> = Vec::new();
    match &content.data {
        ItemData::Login(login) => {
            let mut main = vec![
                Raw::plain("Username", FieldIcon::User, &login.username),
                Raw::secret("Password", FieldIcon::Key, login.password.expose()),
            ];
            if let Some(setup) = &login.totp {
                main.push(Raw {
                    totp: true,
                    ..Raw::secret("Two-factor code", FieldIcon::Clock, setup.expose())
                });
            }
            sections.push((None, main));
            sections.push((
                None,
                login
                    .websites
                    .iter()
                    .map(|site| Raw {
                        link: true,
                        ..Raw::plain("Website", FieldIcon::Globe, &site.url)
                    })
                    .collect(),
            ));
        }
        ItemData::Alias(alias) => sections.push((
            None,
            vec![Raw::plain("Alias address", FieldIcon::Mail, &alias.address).mono()],
        )),
        ItemData::Card(card) => {
            let expires = card
                .expiry
                .map(|expiry| format!("{:02} / {:02}", expiry.month, expiry.year % 100))
                .unwrap_or_default();
            let mut number = Raw::secret("Card number", FieldIcon::Card, "").mono();
            number.display = grouped_card_number(card.number.expose());
            number.copy = Some(Zeroizing::new(
                card.number
                    .expose()
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect(),
            ));
            sections.push((
                None,
                vec![
                    Raw::plain("Cardholder", FieldIcon::User, &card.cardholder),
                    number,
                    Raw::plain("Expires", FieldIcon::Clock, &expires),
                    Raw::secret("Security code", FieldIcon::Shield, card.cvv.expose()),
                    Raw::secret("PIN", FieldIcon::Key, card.pin.expose()),
                ],
            ));
        }
        ItemData::Note(_) => {}
        ItemData::Identity(identity) => {
            let name = join(
                &[
                    &identity.first_name,
                    &identity.middle_name,
                    &identity.last_name,
                ],
                " ",
            );
            let address = &identity.address;
            let street = join(&[&address.line1, &address.line2], ", ");
            sections.push((
                Some("Personal"),
                vec![
                    Raw::plain("Full name", FieldIcon::User, &name),
                    Raw::plain("Email", FieldIcon::Mail, &identity.email),
                    Raw::plain("Phone", FieldIcon::None, &identity.phone),
                ],
            ));
            sections.push((
                Some("Address"),
                vec![
                    Raw::plain("Street", FieldIcon::None, &street),
                    Raw::plain("City", FieldIcon::None, &address.city),
                    Raw::plain("State or region", FieldIcon::None, &address.region),
                    Raw::plain("Postal code", FieldIcon::None, &address.postal_code),
                    Raw::plain("Country", FieldIcon::Globe, &address.country),
                ],
            ));
            sections.push((
                Some("Work"),
                vec![
                    Raw::plain("Company", FieldIcon::None, &identity.company),
                    Raw::plain("Job title", FieldIcon::None, &identity.job_title),
                ],
            ));
            sections.push((
                Some("IDs"),
                identity
                    .documents
                    .iter()
                    .map(|document| {
                        Raw::secret(&document.label, FieldIcon::Id, document.number.expose())
                    })
                    .collect(),
            ));
        }
        ItemData::Password(password) => sections.push((
            None,
            vec![Raw::secret(
                "Password",
                FieldIcon::Key,
                password.password.expose(),
            )],
        )),
        ItemData::Passkey(passkey) => sections.push((
            None,
            vec![
                Raw::plain(
                    "Username",
                    FieldIcon::User,
                    if passkey.user_name.is_empty() {
                        &passkey.user_display_name
                    } else {
                        &passkey.user_name
                    },
                ),
                Raw {
                    link: true,
                    ..Raw::plain("Website", FieldIcon::Globe, &passkey.rp_id)
                },
                Raw::plain("Key ID", FieldIcon::Key, &key_id(&passkey.credential_id)).mono(),
            ],
        )),
    }
    sections.push((
        Some("Custom fields"),
        content
            .custom_fields
            .iter()
            .map(|field| Raw {
                concealed: field.hidden,
                ..Raw::plain(
                    &field.label,
                    if field.hidden {
                        FieldIcon::Eye
                    } else {
                        FieldIcon::Note
                    },
                    field.value.expose(),
                )
            })
            .collect(),
    ));
    for (_, fields) in &mut sections {
        fields.retain(|field| !field.display.is_empty());
    }
    sections.retain(|(_, fields)| !fields.is_empty());
    sections
}

fn raw_field(content: &ItemContent, index: usize) -> Result<Raw, AppError> {
    raw_sections(content)
        .into_iter()
        .flat_map(|(_, fields)| fields)
        .nth(index)
        .ok_or(AppError::Vault(VaultError::NotFound))
}

fn detail(item: &Item, now: i64) -> ItemDetail {
    let content = &item.content;
    let mut index = 0;
    let (mut username_field, mut password_field, mut totp_field) = (None, None, None);
    let sections = raw_sections(content)
        .into_iter()
        .map(|(label, fields)| Section {
            label,
            fields: fields
                .into_iter()
                .map(|raw| {
                    let position = index;
                    index += 1;
                    // The first of each is what the shortcuts copy.
                    if label.is_none() {
                        match raw.label.as_str() {
                            "Username" => username_field = username_field.or(Some(position)),
                            "Password" => password_field = password_field.or(Some(position)),
                            _ if raw.totp => totp_field = Some(position),
                            _ => {}
                        }
                    }
                    Field {
                        index: position,
                        label: raw.label.clone(),
                        icon: raw.icon,
                        value: (!raw.concealed).then(|| raw.display.to_string()),
                        mono: raw.mono,
                        link: raw.link,
                        totp: raw.totp,
                    }
                })
                .collect(),
        })
        .collect();
    let password = match &content.data {
        ItemData::Login(login) => Some((login.password.expose(), login.username.as_str())),
        ItemData::Password(password) => Some((password.password.expose(), "")),
        _ => None,
    };
    ItemDetail {
        id: item.id,
        vault_id: item.vault_id,
        kind: content.kind(),
        title: content.title.clone(),
        favourite: content.favourite,
        sections,
        note: content.notes.clone(),
        strength: password
            .filter(|(password, _)| !password.is_empty())
            .map(|(password, username)| strength::estimate(password, &[username, &content.title])),
        username_field,
        password_field,
        totp_field,
        created_at: item.created_at,
        updated_at: item.updated_at,
        days_left: item
            .trashed_at
            .map(|trashed_at| items::days_left(trashed_at, now)),
        passkey_site: match &content.data {
            ItemData::Passkey(passkey) => Some(passkey.rp_id.clone()),
            _ => None,
        },
    }
}

fn totp_of(content: &ItemContent) -> Option<Totp> {
    match &content.data {
        ItemData::Login(login) => Totp::parse(login.totp.as_ref()?.expose()).ok(),
        _ => None,
    }
}

impl KandadoApp {
    /// Runs `work` with the store and the unlocked session.
    /// Wakes the sync scheduler after a write that worked.
    fn changed<T>(&self, result: Result<T, AppError>) -> Result<T, AppError> {
        if result.is_ok() {
            self.sync.nudge();
        }
        result
    }

    /// A change to the vault: like [`KandadoApp::vault`], then sync soon.
    fn write<T>(
        &self,
        work: impl FnOnce(&Store, &mut Session) -> Result<T, VaultError>,
    ) -> Result<T, AppError> {
        let result = self.vault(work);
        self.changed(result)
    }

    pub(crate) fn vault<T>(
        &self,
        work: impl FnOnce(&Store, &mut Session) -> Result<T, VaultError>,
    ) -> Result<T, AppError> {
        self.with_store_and_session(work)
    }

    // ---- vaults ------------------------------------------------------------

    pub fn vaults(&self) -> Result<Vec<VaultInfo>, AppError> {
        self.vault(|store, session| vaults::list(store, session))
    }

    /// Creates a vault (D-27) and returns its id.
    pub fn create_vault(&self, meta: &VaultMeta) -> Result<Uuid, AppError> {
        self.write(|store, session| vaults::create(store, session, meta, now()))
    }

    pub fn update_vault(&self, id: Uuid, meta: &VaultMeta) -> Result<(), AppError> {
        self.write(|store, session| vaults::update(store, session, id, meta, now()))
    }

    /// Deletes a vault for good, with every item in it, here and, at the next
    /// sync, on the server and every other computer. Refused for the last vault.
    pub fn delete_vault(&self, id: Uuid) -> Result<(), AppError> {
        self.write(|store, session| vaults::delete(store, session, id))
    }

    // ---- lists and search -------------------------------------------------

    /// The rows of a list. Empty while locked.
    pub fn entries(&self, filter: Filter) -> Vec<Entry> {
        self.with_session(|session| session.index().list(filter).into_iter().cloned().collect())
    }

    pub fn entry(&self, id: Uuid) -> Option<Entry> {
        self.with_session(|session| session.index().get(id).cloned())
    }

    /// Search across every vault (D-14), best matches first.
    pub fn search(&self, query: &str) -> Vec<Entry> {
        self.with_session(|session| session.index().search(query).into_iter().cloned().collect())
    }

    /// `(items in all vaults, items in the trash)`.
    pub fn counts(&self) -> (usize, usize) {
        self.with_session(|session| (session.index().count(None), session.index().trash_count()))
    }

    // ---- one item ----------------------------------------------------------

    /// The detail pane's view of an item. Secrets are held back.
    pub fn item_detail(&self, id: Uuid) -> Result<ItemDetail, AppError> {
        let item = self.item(id)?;
        Ok(detail(&item, now()))
    }

    /// The whole item, for a form. Drop it when the form closes.
    pub fn item(&self, id: Uuid) -> Result<Item, AppError> {
        self.vault(|store, session| items::get(store, session, id))
    }

    fn content_at(&self, id: Uuid, revision: Option<i64>) -> Result<ItemContent, AppError> {
        self.vault(|store, session| match revision {
            Some(revision) => history::get(store, session, id, revision),
            None => Ok(items::get(store, session, id)?.content),
        })
    }

    /// The text of one field, for the moment it is revealed. `revision` picks
    /// an earlier version (D-16); `None` is the current one.
    pub fn reveal(
        &self,
        id: Uuid,
        revision: Option<i64>,
        field: usize,
    ) -> Result<Zeroizing<String>, AppError> {
        Ok(raw_field(&self.content_at(id, revision)?, field)?.display)
    }

    /// The current two-factor code of a login, if it has one.
    pub fn totp_code(&self, id: Uuid) -> Result<Option<Code>, AppError> {
        Ok(totp_of(&self.item(id)?.content).map(|totp| totp.code_at(now() as u64)))
    }

    /// Copies one field. Secrets and codes are cleared from the clipboard
    /// after the time set at D-44; plain values such as usernames stay.
    pub fn copy_field(&self, id: Uuid, field: usize) -> Result<Copied, AppError> {
        let content = self.content_at(id, None)?;
        let raw = raw_field(&content, field)?;
        let secret = raw.concealed || raw.totp;
        let text = if raw.totp {
            let totp = totp_of(&content).ok_or(AppError::Vault(VaultError::NotFound))?;
            Zeroizing::new(totp.code_at(now() as u64).digits)
        } else {
            raw.copy.unwrap_or(raw.display)
        };
        self.copy_text(&raw.label, &text, secret)
    }

    /// Copies text that is not a field of an item, such as a password from
    /// the generator.
    pub fn copy_text(&self, label: &str, text: &str, secret: bool) -> Result<Copied, AppError> {
        let delay = if secret {
            self.settings().clipboard_clear.delay()
        } else {
            None
        };
        self.clipboard()
            .copy(text, delay)
            .map_err(|error| AppError::Clipboard(error.to_string()))?;
        Ok(Copied {
            label: label.to_owned(),
            clears_in: delay.map(|delay| delay.as_secs()),
        })
    }

    /// Creates an item and returns its id.
    pub fn create_item(&self, vault: Uuid, content: &ItemContent) -> Result<Uuid, AppError> {
        let mut content = content.clone();
        content.note_password_change(None, now());
        self.write(|store, session| items::create(store, session, vault, &content, now()))
    }

    /// Saves an edit. `vault` moves the item if it differs from where it is.
    pub fn update_item(
        &self,
        id: Uuid,
        vault: Uuid,
        content: &ItemContent,
    ) -> Result<(), AppError> {
        self.write(|store, session| {
            // Monitor needs to know when the password was set (D-36).
            let mut content = content.clone();
            let current = items::get(store, session, id)?.content;
            content.note_password_change(Some(&current), now());
            items::update(store, session, id, &content, now())?;
            items::move_to(store, session, &[id], vault, now())
        })
    }

    pub fn set_favourite(&self, ids: &[Uuid], favourite: bool) -> Result<(), AppError> {
        self.write(|store, session| {
            ids.iter()
                .try_for_each(|&id| items::set_favourite(store, session, id, favourite, now()))
        })
    }

    /// Moves items to another vault (D-34).
    pub fn move_items(&self, ids: &[Uuid], vault: Uuid) -> Result<(), AppError> {
        self.write(|store, session| items::move_to(store, session, ids, vault, now()))
    }

    /// Moves items to the trash. Never asks: the toast offers Undo.
    pub fn trash_items(&self, ids: &[Uuid]) -> Result<(), AppError> {
        self.write(|store, session| items::trash(store, session, ids, now()))
    }

    /// Restore from the trash, and Undo after a move to trash.
    pub fn restore_items(&self, ids: &[Uuid]) -> Result<(), AppError> {
        self.write(|store, session| items::restore(store, session, ids))
    }

    /// Delete forever (D-25). Only items in the trash.
    pub fn delete_forever(&self, ids: &[Uuid]) -> Result<(), AppError> {
        self.write(|store, session| items::delete_forever(store, session, ids))
    }

    /// Empty trash (D-42). Returns how many items were deleted.
    pub fn empty_trash(&self) -> Result<usize, AppError> {
        self.write(items::empty_trash)
    }

    pub fn duplicate_item(&self, id: Uuid) -> Result<Uuid, AppError> {
        self.write(|store, session| items::duplicate(store, session, id, now()))
    }

    // ---- history (D-16) ------------------------------------------------------

    pub fn history(&self, id: Uuid) -> Result<Vec<Version>, AppError> {
        self.vault(|store, session| history::list(store, session, id))
    }

    /// The preview of an earlier version. Secrets are held back; reveal them
    /// with [`KandadoApp::reveal`] and the revision.
    pub fn version_detail(&self, id: Uuid, revision: i64) -> Result<ItemDetail, AppError> {
        let mut item = self.item(id)?;
        item.content = self.content_at(id, Some(revision))?;
        Ok(detail(&item, now()))
    }

    /// Saves an earlier version as the newest one.
    pub fn restore_version(&self, id: Uuid, revision: i64) -> Result<(), AppError> {
        self.write(|store, session| history::restore(store, session, id, revision, now()))
    }
}

/// The first vault, where imported items without a vault of their own go.
pub(crate) fn first_vault(store: &Store, session: &Session) -> Result<Uuid, VaultError> {
    vaults::list(store, session)?
        .first()
        .map(|vault| vault.id)
        .ok_or(VaultError::NotFound)
}
#[cfg(test)]
mod tests {
    use kandado_model::item::{Card, CustomField, Expiry, Login, Website};

    use super::*;

    fn item(content: ItemContent) -> Item {
        Item {
            id: Uuid::from_u128(1),
            vault_id: Uuid::from_u128(2),
            content,
            version_id: Uuid::from_u128(3),
            created_at: 100,
            updated_at: 200,
            trashed_at: None,
        }
    }

    fn login() -> ItemContent {
        ItemContent {
            title: "GitHub".into(),
            notes: "SSH key added 12 Oct.".into(),
            custom_fields: vec![
                CustomField {
                    label: "Recovery email".into(),
                    value: "quiet@example.test".into(),
                    hidden: false,
                    ..CustomField::default()
                },
                CustomField {
                    label: "Security answer".into(),
                    value: "Bahay kubo".into(),
                    hidden: true,
                    ..CustomField::default()
                },
            ],
            data: ItemData::Login(Login {
                username: "juan-dc".into(),
                password: "Tq7#vL2m!Ox0-r9P".into(),
                websites: vec![Website {
                    url: "https://github.com".into(),
                    ..Website::default()
                }],
                totp: Some("otpauth://totp/GitHub:juan?secret=JBSWY3DPEHPK3PXP".into()),
                ..Login::default()
            }),
            ..ItemContent::default()
        }
    }

    #[test]
    fn a_login_detail_holds_no_secret() {
        let detail = detail(&item(login()), 1_000);
        let labels: Vec<Vec<&str>> = detail
            .sections
            .iter()
            .map(|section| section.fields.iter().map(|f| f.label.as_str()).collect())
            .collect();
        assert_eq!(
            labels,
            [
                vec!["Username", "Password", "Two-factor code"],
                vec!["Website"],
                vec!["Recovery email", "Security answer"],
            ]
        );
        assert_eq!(detail.sections[2].label, Some("Custom fields"));
        assert_eq!(
            (
                detail.username_field,
                detail.password_field,
                detail.totp_field
            ),
            (Some(0), Some(1), Some(2))
        );
        assert_eq!(detail.strength.unwrap().label(), "Strong");

        let debug = format!("{detail:?}");
        for secret in ["Tq7#vL2m", "JBSWY3DP", "Bahay kubo"] {
            assert!(!debug.contains(secret), "{secret} is in the view model");
        }
        // Plain values are there.
        assert!(debug.contains("juan-dc") && debug.contains("quiet@example.test"));
        let fields: Vec<&Field> = detail.sections.iter().flat_map(|s| &s.fields).collect();
        assert!(fields[1].concealed() && fields[5].concealed());
        assert!(fields[2].totp && !fields[2].concealed());
        assert!(fields[3].link);
    }

    #[test]
    fn fields_are_revealed_by_index_and_card_numbers_copy_without_spaces() {
        let content = login();
        assert_eq!(
            raw_field(&content, 1).unwrap().display.as_str(),
            "Tq7#vL2m!Ox0-r9P"
        );
        assert_eq!(
            raw_field(&content, 5).unwrap().display.as_str(),
            "Bahay kubo"
        );
        assert!(raw_field(&content, 6).is_err());

        let card = ItemContent::new(
            "BPI Visa",
            ItemData::Card(Card {
                cardholder: "JUAN DELA CRUZ".into(),
                number: "4821009433104821".into(),
                expiry: Some(Expiry {
                    month: 8,
                    year: 2029,
                }),
                cvv: "214".into(),
                ..Card::default()
            }),
        );
        let detail = detail(&item(card.clone()), 1_000);
        let fields = &detail.sections[0].fields;
        // The empty PIN is left out.
        assert_eq!(fields.len(), 4);
        assert_eq!(fields[2].value.as_deref(), Some("08 / 29"));
        assert!(fields[1].concealed() && fields[1].mono);
        let number = raw_field(&card, 1).unwrap();
        assert_eq!(number.display.as_str(), "4821 0094 3310 4821");
        assert_eq!(number.copy.unwrap().as_str(), "4821009433104821");
    }

    #[test]
    fn empty_items_and_trashed_items() {
        let mut note = item(ItemContent {
            title: "Wi-Fi".into(),
            notes: "Network: Bahay-5G".into(),
            ..ItemContent::default()
        });
        note.trashed_at = Some(1_000);
        let detail = detail(&note, 1_000 + 3 * 86_400);
        assert!(detail.sections.is_empty());
        assert_eq!(detail.note, "Network: Bahay-5G");
        assert_eq!(detail.days_left, Some(27));
        assert_eq!(detail.strength, None);
    }
}
