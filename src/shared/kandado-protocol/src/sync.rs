//! Sync: the vault and item rows that travel between a device and the server.
//!
//! Everything a person typed is inside `meta_enc` and `content_enc`, which the
//! server never reads. What the server can see is the plumbing it needs to
//! order and authorise changes: ids, which vault an item is in, its kind
//! (decision A6), timestamps and the trash flag.
//!
//! Every row has a `revision`, drawn from one global sequence on the server.
//! `GET /v1/sync?cursor=N` returns what changed after revision `N`.
//! `POST /v1/sync` sends changes; in a change, `revision` is the revision the
//! device made it against (0 for a row the server has never seen). If the
//! server's row is newer, that change is answered with
//! [`ChangeResult::Conflict`] and the device merges and tries again.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ItemKind, Opaque};

/// The most changes of each kind one `POST /v1/sync` may carry.
pub const MAX_BATCH: usize = 200;

/// The largest `content_enc` or `meta_enc` the server stores, in bytes.
pub const MAX_BLOB: usize = 512 * 1024;

/// A vault. `key_sealed` is the vault key sealed to the caller's public key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncVault {
    pub id: Uuid,
    pub key_sealed: Opaque,
    pub meta_enc: Opaque,
    pub revision: i64,
}

/// An item, or its tombstone (`deleted`), which carries no content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncItem {
    pub id: Uuid,
    pub vault_id: Uuid,
    pub kind: ItemKind,
    /// The item key, wrapped by the vault key.
    pub key_wrapped: Opaque,
    pub content_enc: Opaque,
    /// Names the saved version of the content. Every device that has the same
    /// content has the same id, so versions can be matched across devices.
    pub version_id: Uuid,
    /// Unix seconds, from the device that made the change.
    pub created_at: i64,
    pub updated_at: i64,
    /// Unix seconds, when the item went to the trash.
    pub trashed_at: Option<i64>,
    /// Deleted for good. The server keeps only this marker, so that other
    /// devices learn of it.
    #[serde(default)]
    pub deleted: bool,
    pub revision: i64,
}

/// `GET /v1/sync?cursor=N&limit=M`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullResponse {
    pub vaults: Vec<SyncVault>,
    pub items: Vec<SyncItem>,
    /// Ask again from here. When `more` is false this is the newest revision
    /// the caller has seen.
    pub cursor: i64,
    /// True when the answer was cut short and another pull will return more.
    pub more: bool,
}

/// `POST /v1/sync`
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PushRequest {
    pub vaults: Vec<SyncVault>,
    pub items: Vec<SyncItem>,
}

/// What became of one change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ChangeResult<T> {
    /// Stored. `revision` is the row's new revision.
    Applied { revision: i64 },
    /// The server's row is newer, or is a tombstone. Merge with it and push again.
    Conflict { current: T },
    /// Refused: the row is not the caller's to change, or is not valid.
    Rejected,
}

/// Answers to a [`PushRequest`], in the order the changes were sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushResponse {
    pub vaults: Vec<ChangeResult<SyncVault>>,
    pub items: Vec<ChangeResult<SyncItem>>,
    /// The newest revision the server has for this caller.
    pub cursor: i64,
}

/// One saved version of an item, for its history (D-16).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemVersion {
    pub version_id: Uuid,
    pub content_enc: Opaque,
    pub key_wrapped: Opaque,
    /// Unix seconds.
    pub saved_at: i64,
}

/// `GET /v1/items/{id}/revisions`: every version, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemVersions {
    pub versions: Vec<ItemVersion>,
}

/// What `GET /v1/events` sends: something changed for this user. Pull.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SyncEvent {
    Changed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> SyncItem {
        SyncItem {
            id: Uuid::from_u128(1),
            vault_id: Uuid::from_u128(2),
            kind: ItemKind::Login,
            key_wrapped: Opaque::new(vec![1]),
            content_enc: Opaque::new(vec![2]),
            version_id: Uuid::from_u128(3),
            created_at: 10,
            updated_at: 20,
            trashed_at: None,
            deleted: false,
            revision: 7,
        }
    }

    #[test]
    fn item_wire_format() {
        let json = concat!(
            r#"{"id":"00000000-0000-0000-0000-000000000001","#,
            r#""vault_id":"00000000-0000-0000-0000-000000000002","kind":"login","#,
            r#""key_wrapped":"AQ==","content_enc":"Ag==","#,
            r#""version_id":"00000000-0000-0000-0000-000000000003","#,
            r#""created_at":10,"updated_at":20,"trashed_at":null,"deleted":false,"revision":7}"#,
        );
        assert_eq!(serde_json::to_string(&item()).unwrap(), json);
        assert_eq!(serde_json::from_str::<SyncItem>(json).unwrap(), item());
    }

    #[test]
    fn an_older_client_may_leave_out_deleted() {
        let json = serde_json::to_string(&item())
            .unwrap()
            .replace(r#""deleted":false,"#, "");
        assert!(!serde_json::from_str::<SyncItem>(&json).unwrap().deleted);
    }

    #[test]
    fn change_results_are_tagged() {
        let applied: ChangeResult<SyncItem> = ChangeResult::Applied { revision: 9 };
        assert_eq!(
            serde_json::to_string(&applied).unwrap(),
            r#"{"status":"applied","revision":9}"#
        );
        let rejected: ChangeResult<SyncItem> = ChangeResult::Rejected;
        assert_eq!(
            serde_json::to_string(&rejected).unwrap(),
            r#"{"status":"rejected"}"#
        );
        let conflict = ChangeResult::Conflict { current: item() };
        let json = serde_json::to_string(&conflict).unwrap();
        assert!(json.starts_with(r#"{"status":"conflict","current":{"id""#));
        assert_eq!(
            serde_json::from_str::<ChangeResult<SyncItem>>(&json).unwrap(),
            conflict
        );
    }

    #[test]
    fn events_are_tagged() {
        assert_eq!(
            serde_json::to_string(&SyncEvent::Changed).unwrap(),
            r#"{"type":"changed"}"#
        );
    }
}
