//! Bitwarden: the JSON export (the unencrypted one) and CSV.
//!
//! JSON items have a `type` (1 login, 2 secure note, 3 card, 4 identity,
//! 5 SSH key), `login`, `card` and `identity` objects, custom `fields`, and a
//! `deletedDate` when they are in the trash. Folders are not kept.

use kandado_model::{
    ItemContent, ItemData, Secret,
    item::{Address, Card, CustomField, Document, Identity, Website},
    url_match::MatchMode,
};
use serde_json::Value;

use super::{
    ImportError, Plan, PlannedItem, Source, csv_logins,
    util::{LoginParts, array, custom, humanize, month_year, note, object, text, website},
};

const NAME: &str = "Bitwarden";

pub(super) fn read(bytes: &[u8]) -> Result<Plan, ImportError> {
    let first = bytes.iter().find(|byte| !byte.is_ascii_whitespace());
    if first != Some(&b'{') {
        return csv_logins::read(Source::Bitwarden, bytes);
    }
    let root: Value =
        serde_json::from_slice(bytes).map_err(|_| ImportError::NotThisSource(NAME))?;
    if root.get("encrypted").and_then(Value::as_bool) == Some(true) {
        return Err(ImportError::Unsupported(
            "This Bitwarden export is encrypted. Export again from Bitwarden as \u{201c}.json\u{201d}, not \u{201c}.json (Encrypted)\u{201d}.",
        ));
    }
    let Some(items) = root.get("items").and_then(Value::as_array) else {
        return Err(ImportError::NotThisSource(NAME));
    };
    let mut plan = Plan::new(Source::Bitwarden);
    let folders = array(&root, "folders").len();
    if folders > 0 {
        plan.ignored.push(format!(
            "{folders} {} (Kandado has vaults, not folders)",
            if folders == 1 { "folder" } else { "folders" }
        ));
    }
    let mut linked = 0;
    for item in items {
        read_item(&mut plan, item, &mut linked);
    }
    if linked > 0 {
        plan.ignored.push(format!(
            "{linked} linked custom {} (they point at another field)",
            if linked == 1 { "field" } else { "fields" }
        ));
    }
    Ok(plan)
}

fn match_mode(code: Option<i64>) -> MatchMode {
    match code {
        Some(1..=3) => MatchMode::Host,
        Some(5) => MatchMode::Never,
        _ => MatchMode::Domain,
    }
}

fn custom_fields(item: &Value, linked: &mut usize) -> Vec<CustomField> {
    let mut fields = Vec::new();
    for field in array(item, "fields") {
        let (name, value) = (text(field, "name"), text(field, "value"));
        match field.get("type").and_then(Value::as_i64).unwrap_or(0) {
            3 => *linked += 1,
            1 => fields.push(custom(name, value, true)),
            _ => fields.push(custom(name, value, false)),
        }
    }
    fields
}

fn read_item(plan: &mut Plan, item: &Value, linked: &mut usize) {
    let name = text(item, "name");
    let notes = text(item, "notes");
    let favourite = item
        .get("favorite")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let trashed = !item.get("deletedDate").is_none_or(Value::is_null);
    let mut fields = custom_fields(item, linked);

    let content = match item.get("type").and_then(Value::as_i64).unwrap_or(1) {
        1 => {
            let login = object(item, "login");
            let mut sites: Vec<Website> = Vec::new();
            for uri in array(login, "uris") {
                let mode = match_mode(uri.get("match").and_then(Value::as_i64));
                if let Some(site) = website(text(uri, "uri"), mode) {
                    sites.push(site);
                }
            }
            let urls: Vec<String> = sites.iter().map(|site| site.url.clone()).collect();
            let mut content = LoginParts {
                title: name,
                username: text(login, "username"),
                password: text(login, "password"),
                urls,
                totp: text(login, "totp"),
                notes,
                favourite,
                custom_fields: fields,
            }
            .build();
            // Keep each address's own matching rule.
            if let ItemData::Login(data) = &mut content.data {
                data.websites = sites;
            }
            content
        }
        2 => note(name, notes, fields, favourite),
        3 => {
            let card = object(item, "card");
            let brand = text(card, "brand");
            if !brand.is_empty() {
                fields.insert(0, custom("Card type", brand, false));
            }
            let expiry = month_year(&format!(
                "{}/{}",
                text(card, "expMonth"),
                text(card, "expYear")
            ));
            ItemContent {
                title: name.trim().to_owned(),
                notes: notes.to_owned(),
                favourite,
                custom_fields: fields,
                data: ItemData::Card(Card {
                    cardholder: text(card, "cardholderName").to_owned(),
                    number: Secret::new(text(card, "number")),
                    expiry,
                    cvv: Secret::new(text(card, "code")),
                    ..Card::default()
                }),
                ..ItemContent::default()
            }
        }
        4 => {
            let id = object(item, "identity");
            let get = |key: &str| text(id, key).trim().to_owned();
            let line2 = [get("address2"), get("address3")]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            for key in ["title", "username"] {
                if !get(key).is_empty() {
                    fields.insert(0, custom(humanize(key), get(key), false));
                }
            }
            let mut documents = Vec::new();
            for (key, label) in [
                ("ssn", "Social security number"),
                ("passportNumber", "Passport"),
                ("licenseNumber", "Driver's licence"),
            ] {
                if !get(key).is_empty() {
                    documents.push(Document {
                        label: label.to_owned(),
                        number: Secret::new(get(key)),
                        ..Document::default()
                    });
                }
            }
            ItemContent {
                title: name.trim().to_owned(),
                notes: notes.to_owned(),
                favourite,
                custom_fields: fields,
                data: ItemData::Identity(Box::new(Identity {
                    first_name: get("firstName"),
                    middle_name: get("middleName"),
                    last_name: get("lastName"),
                    email: get("email"),
                    phone: get("phone"),
                    company: get("company"),
                    address: Address {
                        line1: get("address1"),
                        line2,
                        city: get("city"),
                        region: get("state"),
                        postal_code: get("postalCode"),
                        country: get("country"),
                        ..Address::default()
                    },
                    documents,
                    ..Identity::default()
                })),
                ..ItemContent::default()
            }
        }
        _ => {
            // An SSH key or a kind newer than this reader: a note with what
            // it held as hidden fields.
            for key in ["sshKey", "secureNote"] {
                if let Some(map) = item.get(key).and_then(Value::as_object) {
                    for (field, value) in map {
                        if let Some(value) = value.as_str().filter(|value| !value.is_empty()) {
                            fields.push(custom(humanize(field), value, true));
                        }
                    }
                }
            }
            note(name, notes, fields, favourite)
        }
    };
    let mut content = content;
    if content.title.is_empty() {
        content.title = "(no title)".to_owned();
    }
    plan.items.push(PlannedItem {
        content,
        vault: None,
        trashed,
        from_alias: false,
    });
}
