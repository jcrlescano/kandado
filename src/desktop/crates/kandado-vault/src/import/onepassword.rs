//! 1Password: the 1PUX export, a ZIP holding `export.data`.
//!
//! `accounts[].vaults[].items[]`, each with a `categoryUuid` ("001" login,
//! "002" credit card, "003" secure note, "004" identity, "005" password;
//! anything else is another kind of record), `details.loginFields`,
//! `details.sections[].fields[]` and `overview` (title, urls). Items in the
//! trash have `state` "trashed"; archived ones are imported as normal items.

use std::io::{Cursor, Read};

use kandado_model::{
    ItemContent, ItemData, Secret,
    item::{Address, Card, CustomField, Document, Identity, Password},
    url_match::MatchMode,
};
use serde_json::Value;

use super::{
    ImportError, Plan, PlannedItem, Skipped, Source,
    util::{LoginParts, array, custom, humanize, month_year, note, object, text, website},
};

const NAME: &str = "1Password";
const MAX_JSON: u64 = 512 * 1024 * 1024;

pub(super) fn read(bytes: &[u8]) -> Result<Plan, ImportError> {
    let json = if bytes.starts_with(b"PK\x03\x04") {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|_| ImportError::NotThisSource(NAME))?;
        let mut entry = archive
            .by_name("export.data")
            .map_err(|_| ImportError::NotThisSource(NAME))?;
        let mut json = Vec::new();
        (&mut entry)
            .take(MAX_JSON)
            .read_to_end(&mut json)
            .map_err(|_| ImportError::NotThisSource(NAME))?;
        json
    } else {
        bytes.to_vec()
    };
    let root: Value =
        serde_json::from_slice(&json).map_err(|_| ImportError::NotThisSource(NAME))?;
    let Some(accounts) = root.get("accounts").and_then(Value::as_array) else {
        return Err(ImportError::NotThisSource(NAME));
    };
    let mut plan = Plan::new(Source::OnePassword);
    for account in accounts {
        for vault in array(account, "vaults") {
            let vault_name = text(object(vault, "attrs"), "name");
            for item in array(vault, "items") {
                read_item(&mut plan, vault_name, item);
            }
        }
    }
    Ok(plan)
}

/// What a field's value holds, whatever type 1Password gave it.
struct Field {
    label: String,
    id: String,
    value: String,
    concealed: bool,
    totp: bool,
}

/// Reads every field of every section. Attachments are not carried over and
/// are reported.
fn section_fields(details: &Value, plan: &mut Plan, item_label: &str) -> Vec<Field> {
    let mut out = Vec::new();
    for section in array(details, "sections") {
        let heading = text(section, "title");
        for field in array(section, "fields") {
            let title = text(field, "title");
            let label = if heading.is_empty() || title.is_empty() {
                title.to_owned()
            } else {
                format!("{heading}: {title}")
            };
            let value = object(field, "value");
            let Some((kind, inner)) = value.as_object().and_then(|map| map.iter().next()) else {
                continue;
            };
            let (text_value, concealed, totp) = match (kind.as_str(), inner) {
                (
                    "string" | "email" | "url" | "phone" | "gender" | "menu" | "reference"
                    | "creditCardNumber" | "creditCardType",
                    Value::String(s),
                ) => (s.clone(), false, false),
                ("concealed", Value::String(s)) => (s.clone(), true, false),
                ("totp", Value::String(s)) => (s.clone(), true, true),
                ("monthYear", number) => (
                    number.as_i64().map(|n| n.to_string()).unwrap_or_default(),
                    false,
                    false,
                ),
                ("date", number) => (
                    number
                        .as_i64()
                        .map(|seconds| {
                            time::OffsetDateTime::from_unix_timestamp(seconds)
                                .map(|date| date.date().to_string())
                                .unwrap_or_default()
                        })
                        .unwrap_or_default(),
                    false,
                    false,
                ),
                ("address", map) => (
                    ["street", "city", "state", "zip", "country"]
                        .iter()
                        .map(|key| text(map, key))
                        .filter(|part| !part.is_empty())
                        .collect::<Vec<_>>()
                        .join(", "),
                    false,
                    false,
                ),
                ("file", _) => {
                    plan.skipped.push(Skipped {
                        label: format!("{item_label} attachment"),
                        reason: "attached files are not imported".into(),
                    });
                    continue;
                }
                (_, Value::String(s)) => (s.clone(), false, false),
                _ => continue,
            };
            if text_value.is_empty() {
                continue;
            }
            out.push(Field {
                label,
                id: text(field, "id").to_owned(),
                value: text_value,
                concealed,
                totp,
            });
        }
    }
    out
}

fn first<'a>(fields: &'a [Field], ids: &[&str]) -> Option<&'a Field> {
    fields.iter().find(|field| ids.contains(&field.id.as_str()))
}

fn as_custom(field: &Field) -> CustomField {
    custom(field.label.clone(), field.value.clone(), field.concealed)
}

/// Fields not claimed by something else, as custom fields.
fn rest(fields: &[Field], used: &[&str]) -> Vec<CustomField> {
    fields
        .iter()
        .filter(|field| !used.contains(&field.id.as_str()) && !field.totp)
        .map(as_custom)
        .collect()
}

fn read_item(plan: &mut Plan, vault: &str, item: &Value) {
    let overview = object(item, "overview");
    let details = object(item, "details");
    let title = text(overview, "title");
    let label = if title.is_empty() {
        "(no title)"
    } else {
        title
    };
    let notes = text(details, "notesPlain");
    let trashed = text(item, "state") == "trashed";
    let favourite = item.get("favIndex").and_then(Value::as_i64).unwrap_or(0) > 0;
    let fields = section_fields(details, plan, label);

    let content = match text(item, "categoryUuid") {
        "001" => {
            let (mut username, mut password) = ("", "");
            let mut extra: Vec<CustomField> = Vec::new();
            for field in array(details, "loginFields") {
                match text(field, "designation") {
                    "username" => username = text(field, "value"),
                    "password" => password = text(field, "value"),
                    _ if !text(field, "value").is_empty() => extra.push(custom(
                        text(field, "name"),
                        text(field, "value"),
                        text(field, "fieldType") == "P",
                    )),
                    _ => {}
                }
            }
            let mut urls = Vec::new();
            for url in std::iter::once(text(overview, "url")).chain(
                array(overview, "urls")
                    .iter()
                    .map(|entry| text(entry, "url")),
            ) {
                if !url.is_empty() && !urls.iter().any(|seen: &String| seen == url) {
                    urls.push(url.to_owned());
                }
            }
            extra.extend(rest(&fields, &[]));
            let totp = fields
                .iter()
                .find(|field| field.totp)
                .map_or("", |field| field.value.as_str());
            let mut content = LoginParts {
                title,
                username,
                password,
                urls: urls.clone(),
                totp,
                notes,
                favourite,
                custom_fields: extra,
            }
            .build();
            if let ItemData::Login(data) = &mut content.data {
                data.websites = urls
                    .iter()
                    .filter_map(|url| website(url, MatchMode::Domain))
                    .collect();
            }
            content
        }
        "002" => {
            let expiry = first(&fields, &["expiry"]).and_then(|field| month_year(&field.value));
            let used = ["cardholder", "ccnum", "cvv", "expiry", "pin"];
            let mut extra = rest(&fields, &used);
            if let Some(kind) = first(&fields, &["type"]) {
                extra.retain(|field| field.label != kind.label);
                extra.insert(0, custom("Card type", kind.value.clone(), false));
            }
            let get = |id: &str| first(&fields, &[id]).map_or("", |field| field.value.as_str());
            ItemContent {
                title: title.trim().to_owned(),
                notes: notes.to_owned(),
                favourite,
                custom_fields: extra,
                data: ItemData::Card(Card {
                    cardholder: get("cardholder").to_owned(),
                    number: Secret::new(get("ccnum")),
                    expiry,
                    cvv: Secret::new(get("cvv")),
                    pin: Secret::new(get("pin")),
                    ..Card::default()
                }),
                ..ItemContent::default()
            }
        }
        "003" => note(title, notes, rest(&fields, &[]), favourite),
        "004" => {
            let get = |id: &str| first(&fields, &[id]).map_or("", |field| field.value.as_str());
            let used = [
                "firstname",
                "lastname",
                "email",
                "defphone",
                "cellphone",
                "company",
                "jobtitle",
                "address",
            ];
            let phone = if get("defphone").is_empty() {
                get("cellphone")
            } else {
                get("defphone")
            };
            ItemContent {
                title: title.trim().to_owned(),
                notes: notes.to_owned(),
                favourite,
                custom_fields: rest(&fields, &used),
                data: ItemData::Identity(Box::new(Identity {
                    first_name: get("firstname").to_owned(),
                    last_name: get("lastname").to_owned(),
                    email: get("email").to_owned(),
                    phone: phone.to_owned(),
                    company: get("company").to_owned(),
                    job_title: get("jobtitle").to_owned(),
                    address: Address {
                        // 1Password keeps an address as one value.
                        line1: get("address").to_owned(),
                        ..Address::default()
                    },
                    documents: Vec::<Document>::new(),
                    ..Identity::default()
                })),
                ..ItemContent::default()
            }
        }
        "005" => {
            let password = text(details, "password");
            ItemContent {
                title: title.trim().to_owned(),
                notes: notes.to_owned(),
                favourite,
                custom_fields: rest(&fields, &[]),
                data: ItemData::Password(Password {
                    password: Secret::new(password),
                    ..Password::default()
                }),
                ..ItemContent::default()
            }
        }
        other => {
            // Documents, licences, bank accounts, servers and the rest: a
            // note with every field listed.
            let mut extra = rest(&fields, &[]);
            if !other.is_empty() {
                extra.push(custom("1Password category", humanize(other), false));
            }
            for field in array(details, "loginFields") {
                if !text(field, "value").is_empty() {
                    extra.push(custom(
                        text(field, "name"),
                        text(field, "value"),
                        text(field, "fieldType") == "P",
                    ));
                }
            }
            note(title, notes, extra, favourite)
        }
    };
    let mut content = content;
    if content.title.is_empty() {
        content.title = "(no title)".to_owned();
    }
    plan.items.push(PlannedItem {
        content,
        vault: (!vault.is_empty()).then(|| vault.to_owned()),
        trashed,
        from_alias: false,
    });
}
