//! Proton Pass: the ZIP with `data.json`, the PGP-encrypted export, and CSV.
//!
//! `data.json` holds `vaults`, an object of vault id to `{ name, items }`.
//! Each item has `data.metadata` (name, note), `data.type`, `data.content`,
//! `data.extraFields` and `state` (1 active, 2 trashed).
//!
//! Mapping: login, note, credit card and identity become the same Kandado
//! kinds. An alias becomes a note holding its address. A kind Kandado has no
//! place for becomes a note with its fields listed, hidden. Passkeys are
//! reported as skipped, since they arrive with phase 8.

use std::io::{Cursor, Read};

use kandado_model::{
    ItemContent, ItemData, Secret,
    item::{Address, Card, CustomField, Document, Identity},
};
use serde_json::Value;

use super::{
    ImportError, Plan, PlannedItem, Skipped, Source, csv_logins,
    util::{LoginParts, array, custom, humanize, month_year, note, object, text},
};

const NAME: &str = "Proton Pass";
/// The least a ZIP of Proton's can be: one entry. Anything larger than this
/// many bytes for `data.json` is refused, so a hostile ZIP cannot fill memory.
const MAX_JSON: u64 = 512 * 1024 * 1024;

pub(super) fn read(bytes: &[u8], passphrase: Option<&str>) -> Result<Plan, ImportError> {
    read_inner(bytes, passphrase, 0)
}

fn read_inner(bytes: &[u8], passphrase: Option<&str>, depth: u8) -> Result<Plan, ImportError> {
    if depth > 2 {
        return Err(ImportError::NotThisSource(NAME));
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return read_json(&json_from_zip(bytes)?);
    }
    if is_pgp(bytes) {
        let passphrase = passphrase.ok_or(ImportError::NeedsPassphrase)?;
        let inner = decrypt(bytes, passphrase)?;
        return read_inner(&inner, Some(passphrase), depth + 1);
    }
    let first = bytes.iter().find(|byte| !byte.is_ascii_whitespace());
    if first == Some(&b'{') {
        return read_json(bytes);
    }
    // Otherwise it can only be the CSV export.
    csv_logins::read(Source::ProtonPass, bytes)
}

fn is_pgp(bytes: &[u8]) -> bool {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(0);
    let bytes = &bytes[start..];
    bytes.starts_with(b"-----BEGIN PGP MESSAGE-----")
        // A binary OpenPGP packet starts with a byte with the top bit set.
        || bytes.first().is_some_and(|byte| byte & 0x80 != 0 && *byte != b'P')
}

fn json_from_zip(bytes: &[u8]) -> Result<Vec<u8>, ImportError> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| ImportError::NotThisSource(NAME))?;
    for index in 0..archive.len() {
        let Ok(entry) = archive.by_index(index) else {
            continue;
        };
        if !entry
            .name()
            .is_ok_and(|name| name.to_lowercase().ends_with("data.json"))
        {
            continue;
        }
        let mut json = Vec::new();
        entry
            .take(MAX_JSON)
            .read_to_end(&mut json)
            .map_err(|_| ImportError::NotThisSource(NAME))?;
        return Ok(json);
    }
    Err(ImportError::NotThisSource(NAME))
}

/// Opens a PGP message encrypted with a password.
fn decrypt(bytes: &[u8], passphrase: &str) -> Result<Vec<u8>, ImportError> {
    use pgp::composed::Message;

    let wrong = |_: pgp::errors::Error| ImportError::WrongPassphrase;
    let parsed = if bytes
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| *byte == b'-')
    {
        Message::from_armor(Cursor::new(bytes)).map(|(message, _)| message)
    } else {
        Message::from_bytes(Cursor::new(bytes))
    }
    .map_err(|_| ImportError::NotThisSource(NAME))?;
    let mut message = parsed
        .decrypt_with_password(&passphrase.into())
        .map_err(wrong)?;
    message
        .as_data_vec()
        .map_err(|_| ImportError::WrongPassphrase)
}

fn read_json(json: &[u8]) -> Result<Plan, ImportError> {
    let root: Value = serde_json::from_slice(json).map_err(|_| ImportError::NotThisSource(NAME))?;
    if root.get("encrypted").and_then(Value::as_bool) == Some(true) {
        return Err(ImportError::Unsupported(
            "This export is encrypted inside. Export again from Proton Pass with the ZIP option, or with a password to get a PGP file.",
        ));
    }
    let Some(vaults) = root.get("vaults").and_then(Value::as_object) else {
        return Err(ImportError::NotThisSource(NAME));
    };
    let mut plan = Plan::new(Source::ProtonPass);
    for vault in vaults.values() {
        let vault_name = text(vault, "name");
        for item in array(vault, "items") {
            read_item(&mut plan, vault_name, item);
        }
    }
    Ok(plan)
}

/// The identity fields Proton has that map to a Kandado field. Every other
/// non-empty text field becomes a custom field.
const IDENTITY_USED: &[&str] = &[
    "fullName",
    "firstName",
    "middleName",
    "lastName",
    "email",
    "phoneNumber",
    "organization",
    "streetAddress",
    "zipOrPostalCode",
    "city",
    "stateOrProvince",
    "countryOrRegion",
    "floor",
    "county",
    "socialSecurityNumber",
    "passportNumber",
    "licenseNumber",
];

fn extra_fields(item_data: &Value) -> Vec<CustomField> {
    let mut fields = Vec::new();
    for field in array(item_data, "extraFields") {
        let label = text(field, "fieldName");
        let data = object(field, "data");
        let value = match text(field, "type") {
            "totp" => text(data, "totpUri"),
            _ => text(data, "content"),
        };
        let hidden = matches!(text(field, "type"), "hidden" | "totp");
        if !label.is_empty() || !value.is_empty() {
            fields.push(custom(label, value, hidden));
        }
    }
    fields
}

fn identity(content: &Value) -> ItemData {
    let get = |key: &str| text(content, key).trim().to_owned();
    let (mut first, last) = (get("firstName"), get("lastName"));
    if first.is_empty() && last.is_empty() {
        first = get("fullName");
    }
    let line2 = ["floor", "county", "extraAddressDetails"]
        .iter()
        .map(|key| get(key))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let mut documents = Vec::new();
    for (key, label) in [
        ("socialSecurityNumber", "Social security number"),
        ("passportNumber", "Passport"),
        ("licenseNumber", "Driver's licence"),
    ] {
        let number = get(key);
        if !number.is_empty() {
            documents.push(Document {
                label: label.to_owned(),
                number: Secret::new(number),
                ..Document::default()
            });
        }
    }
    ItemData::Identity(Box::new(Identity {
        first_name: first,
        middle_name: get("middleName"),
        last_name: last,
        email: get("email"),
        phone: get("phoneNumber"),
        company: get("organization"),
        address: Address {
            line1: get("streetAddress"),
            line2,
            city: get("city"),
            region: get("stateOrProvince"),
            postal_code: get("zipOrPostalCode"),
            country: get("countryOrRegion"),
            ..Address::default()
        },
        documents,
        ..Identity::default()
    }))
}

/// Everything in an identity that has no field of its own, as custom fields.
fn identity_extras(content: &Value) -> Vec<CustomField> {
    let mut fields = Vec::new();
    let Some(map) = content.as_object() else {
        return fields;
    };
    for (key, value) in map {
        if IDENTITY_USED.contains(&key.as_str()) {
            continue;
        }
        match value {
            Value::String(text) if !text.is_empty() => {
                fields.push(custom(humanize(key), text.clone(), false));
            }
            // Sections the person added: "extraSections", each with fields.
            Value::Array(sections) if key == "extraSections" => {
                for section in sections {
                    let title = text(section, "sectionName");
                    for field in array(section, "sectionFields") {
                        let label = text(field, "fieldName");
                        let data = object(field, "data");
                        let value = if text(field, "type") == "totp" {
                            text(data, "totpUri")
                        } else {
                            text(data, "content")
                        };
                        if !value.is_empty() {
                            let label = if title.is_empty() {
                                label.to_owned()
                            } else {
                                format!("{title}: {label}")
                            };
                            fields.push(custom(label, value, text(field, "type") != "text"));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    fields
}

fn read_item(plan: &mut Plan, vault: &str, item: &Value) {
    let data = object(item, "data");
    let metadata = object(data, "metadata");
    let (name, notes) = (text(metadata, "name"), text(metadata, "note"));
    let content = object(data, "content");
    let trashed = item.get("state").and_then(Value::as_i64) == Some(2);
    let label = if name.is_empty() { "(no title)" } else { name };
    let mut custom_fields = extra_fields(data);
    let pinned = item.get("pinned").and_then(Value::as_bool).unwrap_or(false);
    let mut from_alias = false;

    let item_content = match text(data, "type") {
        "login" => {
            for passkey in array(content, "passkeys") {
                let site = text(passkey, "domain");
                plan.skipped.push(Skipped {
                    label: format!(
                        "{label} passkey{}",
                        if site.is_empty() {
                            String::new()
                        } else {
                            format!(" ({site})")
                        }
                    ),
                    reason: "passkeys can be imported once Kandado supports them".into(),
                });
            }
            let (username, email) = (text(content, "itemUsername"), text(content, "itemEmail"));
            let username = if username.is_empty() { email } else { username };
            if !email.is_empty() && email != username {
                custom_fields.insert(0, custom("Email", email, false));
            }
            let parts = LoginParts {
                title: name,
                username,
                password: text(content, "password"),
                urls: array(content, "urls")
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
                totp: text(content, "totpUri"),
                notes,
                favourite: pinned,
                custom_fields,
            };
            parts.build()
        }
        "note" => note(name, notes, custom_fields, pinned),
        "alias" => {
            // The address belongs to Proton, so it cannot move: it is kept as
            // a note, and counted as such in the result.
            from_alias = true;
            let address = item.get("aliasEmail").and_then(Value::as_str).unwrap_or("");
            let text = if notes.is_empty() {
                format!("Email alias: {address}")
            } else {
                format!("Email alias: {address}\n\n{notes}")
            };
            note(name, &text, custom_fields, pinned)
        }
        "creditCard" => {
            let expiry = month_year(text(content, "expirationDate"));
            let card_type = text(content, "cardType");
            if !card_type.is_empty() {
                custom_fields.insert(0, custom("Card type", card_type, false));
            }
            ItemContent {
                title: name.trim().to_owned(),
                notes: notes.to_owned(),
                favourite: pinned,
                custom_fields,
                data: ItemData::Card(Card {
                    cardholder: text(content, "cardholderName").to_owned(),
                    number: Secret::new(text(content, "number")),
                    expiry,
                    cvv: Secret::new(text(content, "verificationNumber")),
                    pin: Secret::new(text(content, "pin")),
                    ..Card::default()
                }),
                ..ItemContent::default()
            }
        }
        "identity" => {
            custom_fields.extend(identity_extras(content));
            ItemContent {
                title: name.trim().to_owned(),
                notes: notes.to_owned(),
                favourite: pinned,
                custom_fields,
                data: identity(content),
                ..ItemContent::default()
            }
        }
        _ => {
            // A kind Kandado has no place for: a note, with what it held
            // listed as hidden fields. Nothing is dropped.
            if let Some(map) = content.as_object() {
                for (key, value) in map {
                    if let Some(value) = value.as_str().filter(|value| !value.is_empty()) {
                        custom_fields.push(custom(humanize(key), value, true));
                    }
                }
            }
            note(name, notes, custom_fields, pinned)
        }
    };

    let mut item_content = item_content;
    if item_content.title.is_empty() {
        item_content.title = "(no title)".to_owned();
    }
    plan.items.push(PlannedItem {
        content: item_content,
        vault: (!vault.is_empty()).then(|| vault.to_owned()),
        trashed,
        from_alias,
    });
}
