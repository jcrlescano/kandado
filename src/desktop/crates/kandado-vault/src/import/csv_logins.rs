//! CSV exports of logins: Proton Pass, Bitwarden, LastPass, Chrome, Edge,
//! Firefox, Safari and any file with a header row.
//!
//! The header row says which column is which. Names differ between managers
//! (`login_uri`, `url`, `website`), so each field has a list of the names it
//! goes by. A row with nothing worth keeping is skipped and listed.

use std::collections::HashMap;

use super::{
    ImportError, Plan, PlannedItem, Skipped, Source,
    util::{LoginParts, custom, note},
};

/// Column names, lower-case, for each field.
const TITLE: &[&str] = &["name", "title", "site", "account", "login_name"];
const URL: &[&str] = &[
    "url",
    "login_uri",
    "website",
    "web site",
    "uri",
    "origin",
    "address",
];
const USERNAME: &[&str] = &[
    "username",
    "login_username",
    "user name",
    "login",
    "user",
    "login name",
];
const EMAIL: &[&str] = &["email", "e-mail"];
const PASSWORD: &[&str] = &["password", "login_password", "pass"];
const NOTES: &[&str] = &["note", "notes", "extra", "comments", "comment"];
const TOTP: &[&str] = &["totp", "login_totp", "otpauth", "otp", "otp auth"];
const FOLDER: &[&str] = &["folder", "grouping", "group"];
const VAULT: &[&str] = &["vault"];
const TYPE: &[&str] = &["type"];
const FAVOURITE: &[&str] = &["favorite", "favourite", "fav"];
const FIELDS: &[&str] = &["fields"];

fn names(source: Source) -> &'static str {
    source.name()
}

struct Columns(HashMap<String, usize>);

impl Columns {
    fn of(headers: &csv::StringRecord) -> Self {
        let mut map = HashMap::new();
        for (index, header) in headers.iter().enumerate() {
            let name = header.trim().trim_start_matches('\u{feff}').to_lowercase();
            map.entry(name).or_insert(index);
        }
        Self(map)
    }

    fn find(&self, names: &[&str]) -> Option<usize> {
        names.iter().find_map(|name| self.0.get(*name).copied())
    }
}

fn cell(row: &csv::StringRecord, column: Option<usize>) -> &str {
    column.and_then(|index| row.get(index)).unwrap_or("")
}

/// `name: value` lines, as Bitwarden writes custom fields.
fn parse_fields(text: &str) -> Vec<kandado_model::item::CustomField> {
    text.lines()
        .filter_map(|line| {
            let (name, value) = line.split_once(": ")?;
            Some(custom(name.trim(), value.trim(), false))
        })
        .collect()
}

pub(super) fn read(source: Source, bytes: &[u8]) -> Result<Plan, ImportError> {
    let not_this = || ImportError::NotThisSource(names(source));
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::None)
        .from_reader(bytes);
    let headers = reader.headers().map_err(|_| not_this())?.clone();
    let columns = Columns::of(&headers);
    let (title, url, username, email, password, notes, totp) = (
        columns.find(TITLE),
        columns.find(URL),
        columns.find(USERNAME),
        columns.find(EMAIL),
        columns.find(PASSWORD),
        columns.find(NOTES),
        columns.find(TOTP),
    );
    // A login export has at least a password or an address column.
    if password.is_none() && url.is_none() && title.is_none() {
        return Err(not_this());
    }
    let (folder, vault, kind, favourite, fields) = (
        columns.find(FOLDER),
        columns.find(VAULT),
        columns.find(TYPE),
        columns.find(FAVOURITE),
        columns.find(FIELDS),
    );

    let mut plan = Plan::new(source);
    let mut folders: Vec<String> = Vec::new();
    for (index, row) in reader.records().enumerate() {
        let Ok(row) = row else {
            plan.skipped.push(Skipped {
                label: format!("Row {}", index + 2),
                reason: "the row could not be read".into(),
            });
            continue;
        };
        if row.iter().all(|cell| cell.trim().is_empty()) {
            continue;
        }
        let folder_name = cell(&row, folder).trim();
        if !folder_name.is_empty() && !folders.iter().any(|seen| seen == folder_name) {
            folders.push(folder_name.to_owned());
        }
        let name = cell(&row, title);
        let label = if name.trim().is_empty() {
            format!("Row {}", index + 2)
        } else {
            name.to_owned()
        };
        let row_kind = cell(&row, kind).trim().to_lowercase();
        let (username_value, email_value) = (cell(&row, username), cell(&row, email));
        let mut extra = parse_fields(cell(&row, fields));
        let (username_value, email_extra) = if username_value.trim().is_empty() {
            (email_value, "")
        } else {
            (username_value, email_value)
        };
        if !email_extra.trim().is_empty() && email_extra != username_value {
            extra.insert(0, custom("Email", email_extra, false));
        }
        let urls: Vec<String> = cell(&row, url)
            .lines()
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(str::to_owned)
            .collect();
        let favourite = matches!(
            cell(&row, favourite).trim().to_lowercase().as_str(),
            "1" | "true" | "yes"
        );
        // LastPass writes secure notes as rows whose address is "http://sn".
        let secure_note = urls.len() == 1 && urls[0] == "http://sn";

        let content = if row_kind == "note" || secure_note {
            if cell(&row, notes).trim().is_empty() && name.trim().is_empty() {
                plan.skipped.push(Skipped {
                    label,
                    reason: "the note has no title or text".into(),
                });
                continue;
            }
            note(name, cell(&row, notes), extra, favourite)
        } else if !row_kind.is_empty() && row_kind != "login" {
            plan.skipped.push(Skipped {
                label,
                reason: format!("\u{201c}{row_kind}\u{201d} items are not in a CSV export"),
            });
            continue;
        } else {
            let parts = LoginParts {
                title: name,
                username: username_value,
                password: cell(&row, password),
                urls,
                totp: cell(&row, totp),
                notes: cell(&row, notes),
                favourite,
                custom_fields: extra,
            };
            if parts.is_empty() {
                plan.skipped.push(Skipped {
                    label,
                    reason: "no title, address, sign-in or note".into(),
                });
                continue;
            }
            parts.build()
        };
        let vault_name = cell(&row, vault).trim();
        plan.items.push(PlannedItem {
            content,
            vault: (!vault_name.is_empty()).then(|| vault_name.to_owned()),
            trashed: false,
            from_alias: false,
        });
    }
    if !folders.is_empty() {
        plan.ignored.push(format!(
            "{} {} (Kandado has vaults, not folders)",
            folders.len(),
            if folders.len() == 1 {
                "folder"
            } else {
                "folders"
            }
        ));
    }
    Ok(plan)
}
