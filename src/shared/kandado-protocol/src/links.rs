//! Secure links (D-32, D-33): one item shared with anyone who has the link.
//!
//! The device encrypts a snapshot of the item with a key only the link holds
//! (see `kandado_model::link`) and sends the ciphertext here. The server keeps
//! it with an expiry and a view limit, and hands it out to whoever opens the
//! link until either runs out or the owner revokes it. It never sees the key.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

pub use kandado_model::link::{
    EXPIRY_CHOICES as LINK_EXPIRY_CHOICES, VIEW_CHOICES as LINK_VIEW_CHOICES,
};

use crate::Opaque;

/// The largest payload the server stores for one link, in bytes. A snapshot
/// of one item is far smaller.
pub const MAX_LINK_PAYLOAD: usize = 32 * 1024;

/// `POST /v1/links`. The device chooses the id, because the payload is
/// encrypted to it before the server has seen anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateLink {
    pub id: Uuid,
    /// The item the snapshot was made from, so the owner's list can name it.
    pub item_id: Uuid,
    pub payload_enc: Opaque,
    /// Seconds until the link stops working. One of [`LINK_EXPIRY_CHOICES`].
    pub expires_in: i64,
    /// How many times it may be opened; `None` for no limit. One of
    /// [`LINK_VIEW_CHOICES`].
    pub max_views: Option<i32>,
}

/// The answer to [`CreateLink`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkCreated {
    /// The link without its key: `{server}/l/{id}`. The device adds `#` and
    /// the key, which the server never sees.
    pub url: String,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
}

/// Whether a link still works.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    Active,
    Expired,
    /// Opened as many times as it allows.
    UsedUp,
    Revoked,
}

/// One of the caller's links, for the list (D-33). No payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkInfo {
    pub id: Uuid,
    pub item_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    pub max_views: Option<i32>,
    pub views: i32,
    pub state: LinkState,
}

/// `GET /v1/links`: newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkList {
    pub links: Vec<LinkInfo>,
}

/// `POST /v1/public/links/{id}/open`: counts one view and returns the payload.
/// Every link that does not work any more answers 404, whatever the reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenedLink {
    pub payload_enc: Opaque,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    /// Opens left after this one; `None` for no limit.
    pub views_left: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_wire_format() {
        let create = CreateLink {
            id: Uuid::from_u128(1),
            item_id: Uuid::from_u128(2),
            payload_enc: Opaque::new(vec![1, 2]),
            expires_in: 86_400,
            max_views: Some(1),
        };
        let json = concat!(
            r#"{"id":"00000000-0000-0000-0000-000000000001","#,
            r#""item_id":"00000000-0000-0000-0000-000000000002","#,
            r#""payload_enc":"AQI=","expires_in":86400,"max_views":1}"#
        );
        assert_eq!(serde_json::to_string(&create).unwrap(), json);
        assert_eq!(serde_json::from_str::<CreateLink>(json).unwrap(), create);
    }

    #[test]
    fn states_use_snake_case() {
        assert_eq!(
            serde_json::to_string(&LinkState::UsedUp).unwrap(),
            r#""used_up""#
        );
    }
}
