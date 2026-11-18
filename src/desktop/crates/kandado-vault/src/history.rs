//! An item's earlier versions (D-16). Every saved edit keeps the version it
//! replaced, encrypted with the same item key. Restoring a version saves it
//! as the newest one, so nothing is ever lost by restoring.
//!
//! The server keeps every version of an item too, so a second computer can
//! show them. [`merge_remote`] brings the ones this computer lacks.
//!
//! A version is named by a number that is stable on this computer: `0` is the
//! current version, anything else is an earlier one. Versions are matched
//! across computers by their version id, which is never shown.

use kandado_model::{ItemContent, ItemData};
use kandado_protocol::ItemVersion;
use uuid::Uuid;

use crate::{Session, Store, VaultError, items};

/// The number of the current version of every item.
pub const CURRENT: i64 = 0;

/// One version in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// [`CURRENT`], or the number of an earlier version.
    pub revision: i64,
    /// When this version was saved, Unix seconds.
    pub saved_at: i64,
    /// The version the item has now.
    pub current: bool,
    /// What changed from the version before, such as "Password changed".
    pub summary: String,
}

/// What changed between two versions, in the design's words.
fn summarise(before: &ItemContent, after: &ItemContent) -> String {
    let mut changes: Vec<&str> = Vec::new();
    if before.title != after.title {
        changes.push("Title changed");
    }
    match (&before.data, &after.data) {
        (ItemData::Login(old), ItemData::Login(new)) => {
            if old.username != new.username {
                changes.push("Username changed");
            }
            if old.password != new.password {
                changes.push("Password changed");
            }
            match (old.websites.len(), new.websites.len()) {
                (a, b) if a < b => changes.push("Website added"),
                (a, b) if a > b => changes.push("Website removed"),
                _ if old.websites != new.websites => changes.push("Website changed"),
                _ => {}
            }
            match (&old.totp, &new.totp) {
                (None, Some(_)) => changes.push("Two-factor code added"),
                (Some(_), None) => changes.push("Two-factor code removed"),
                (Some(a), Some(b)) if a != b => changes.push("Two-factor code changed"),
                _ => {}
            }
        }
        (ItemData::Password(old), ItemData::Password(new)) => {
            if old.password != new.password {
                changes.push("Password changed");
            }
        }
        (old, new) if old != new => changes.push("Details changed"),
        _ => {}
    }
    if before.notes != after.notes {
        changes.push("Note changed");
    }
    match (before.custom_fields.len(), after.custom_fields.len()) {
        (a, b) if a < b => changes.push("Custom field added"),
        (a, b) if a > b => changes.push("Custom field removed"),
        _ if before.custom_fields != after.custom_fields => changes.push("Custom field changed"),
        _ => {}
    }
    match changes.as_slice() {
        [] => "Edited".to_owned(),
        [one] => (*one).to_owned(),
        [first, rest @ ..] => format!("{first} and {} more", rest.len()),
    }
}

/// Every version of an item, newest first, starting with the current one.
pub fn list(store: &Store, session: &Session, item: Uuid) -> Result<Vec<Version>, VaultError> {
    let (row, key) = items::row_and_key(store, session, item)?;
    // Newest first: the current content, then the stored versions. A stored
    // copy of the current version (from the server) is left out.
    let mut versions = vec![(
        CURRENT,
        row.updated_at,
        items::decrypt(&key, item, &row.content_enc)?,
    )];
    for stored in store.revisions(item)? {
        if stored.version_id == row.version_id {
            continue;
        }
        // One that cannot be read (damaged, or from a newer Kandado) is skipped.
        if let Ok(content) = items::decrypt(&key, item, &stored.content_enc) {
            versions.push((stored.local_id, stored.saved_at, content));
        }
    }
    Ok(versions
        .iter()
        .enumerate()
        .map(|(position, (revision, saved_at, content))| Version {
            revision: *revision,
            saved_at: *saved_at,
            current: position == 0,
            summary: match versions.get(position + 1) {
                Some((_, _, before)) => summarise(before, content),
                None => "Created".to_owned(),
            },
        })
        .collect())
}

/// The content of one version, for the preview.
pub fn get(
    store: &Store,
    session: &Session,
    item: Uuid,
    revision: i64,
) -> Result<ItemContent, VaultError> {
    let (row, key) = items::row_and_key(store, session, item)?;
    if revision == CURRENT {
        return items::decrypt(&key, item, &row.content_enc);
    }
    let found = store
        .revisions(item)?
        .into_iter()
        .find(|stored| stored.local_id == revision)
        .ok_or(VaultError::NotFound)?;
    items::decrypt(&key, item, &found.content_enc)
}

/// Saves an earlier version as the newest one. The star stays as it is now.
pub fn restore(
    store: &Store,
    session: &mut Session,
    item: Uuid,
    revision: i64,
    now: i64,
) -> Result<(), VaultError> {
    if revision == CURRENT {
        return Ok(());
    }
    let mut content = get(store, session, item, revision)?;
    content.favourite = items::get(store, session, item)?.content.favourite;
    items::update(store, session, item, &content, now)
}

/// Keeps the versions the server has that this computer lacks, so the list
/// shows the whole history. Returns how many were new.
pub fn merge_remote(
    store: &Store,
    session: &Session,
    item: Uuid,
    remote: &[ItemVersion],
) -> Result<usize, VaultError> {
    let (row, key) = items::row_and_key(store, session, item)?;
    let known: Vec<Uuid> = store
        .revisions(item)?
        .into_iter()
        .map(|stored| stored.version_id)
        .chain([row.version_id])
        .collect();
    let mut added = 0;
    for version in remote {
        if known.contains(&version.version_id) {
            continue;
        }
        // Only what this item's key opens is kept.
        if items::decrypt(&key, item, version.content_enc.as_bytes()).is_err() {
            continue;
        }
        store.add_revision(
            item,
            version.version_id,
            version.content_enc.as_bytes(),
            version.saved_at,
        )?;
        added += 1;
    }
    Ok(added)
}
