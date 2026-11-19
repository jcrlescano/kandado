//! Secure links (D-31, D-32, D-33): share one item with anyone, through a
//! link that expires.
//!
//! The item is turned into a snapshot and encrypted here with a new key
//! (`kandado_model::link`). Only the ciphertext goes to the server. The key is
//! put in the link's fragment and exists nowhere else: it is shown once, in
//! [`NewLink`], and cannot be recovered afterwards.

use std::fmt;

use kandado_model::{
    ItemData, ItemKind,
    link::{self, LinkSnapshot},
};
use kandado_protocol::{
    CreateLink, LINK_EXPIRY_CHOICES, LINK_VIEW_CHOICES, LinkInfo, Opaque, ProblemCode,
};
use kandado_vault::ApiError;
use time::OffsetDateTime;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{AppError, KandadoApp, vault::Copied};

/// What the person chose in D-32.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkOptions {
    /// Index into [`LINK_EXPIRY_CHOICES`]: 1 hour, 1 day, 7 days, 30 days.
    pub expiry: usize,
    /// Index into [`LINK_VIEW_CHOICES`]: once, 5 times, 10 times, no limit.
    pub views: usize,
    /// Put the two-factor setup key in the link.
    pub include_totp: bool,
}

impl Default for LinkOptions {
    /// D-32's defaults: one day, once, no two-factor code.
    fn default() -> Self {
        Self {
            expiry: 1,
            views: 0,
            include_totp: false,
        }
    }
}

/// A link just made. It holds the key, so it is wiped when dropped and never
/// printed. Show it, let it be copied, then let it go.
pub struct NewLink {
    pub id: Uuid,
    pub url: Zeroizing<String>,
    pub expires_at: OffsetDateTime,
    pub max_views: Option<i32>,
}

impl fmt::Debug for NewLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NewLink")
            .field("id", &self.id)
            .field("url", &"..")
            .finish()
    }
}

/// A row of the secure links list (D-33): the server's facts and the item's
/// title from this computer. `title` is `None` when the item is no longer here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRow {
    pub info: LinkInfo,
    pub title: Option<String>,
    pub kind: Option<ItemKind>,
}

impl KandadoApp {
    /// Whether an item may be shared by link: not a passkey (its private key
    /// would leave Kandado) and not in the trash.
    pub fn can_share(&self, item: Uuid) -> bool {
        self.entry(item)
            .is_some_and(|entry| entry.kind != ItemKind::Passkey && entry.trashed_at.is_none())
    }

    /// Whether the item has a two-factor setup key, so D-32 offers to include it.
    pub fn has_totp(&self, item: Uuid) -> bool {
        self.item(item).is_ok_and(|item| {
            matches!(&item.content.data, ItemData::Login(login)
                if login.totp.as_ref().is_some_and(|totp| !totp.is_empty()))
        })
    }

    /// Makes a secure link to an item (D-32). If the server has not seen the
    /// item yet, syncs once and tries again.
    pub async fn create_link(&self, item: Uuid, options: LinkOptions) -> Result<NewLink, AppError> {
        let expires_in = *LINK_EXPIRY_CHOICES
            .get(options.expiry)
            .ok_or(AppError::WrongState)?;
        let max_views = *LINK_VIEW_CHOICES
            .get(options.views)
            .ok_or(AppError::WrongState)?;
        let id = Uuid::now_v7();
        let (payload, key) = {
            let content = self.item(item)?.content;
            let snapshot = LinkSnapshot::of(&content, options.include_totp)?;
            link::seal(&snapshot, *id.as_bytes())?
        };
        let request = CreateLink {
            id,
            item_id: item,
            payload_enc: Opaque::new(payload),
            expires_in,
            max_views,
        };
        let created = match self.api().create_link(&request).await {
            Err(ApiError::Problem(problem)) if problem.code == ProblemCode::NotFound => {
                // A new item may not have reached the server yet.
                self.sync_now().await.map_err(|error| match error {
                    kandado_vault::sync::SyncError::Api(error) => AppError::Api(error),
                    kandado_vault::sync::SyncError::Vault(error) => AppError::Vault(error),
                })?;
                self.api().create_link(&request).await?
            }
            other => other?,
        };
        Ok(NewLink {
            id,
            url: Zeroizing::new(format!(
                "{}#{}",
                created.url,
                link::key_to_fragment(&key).as_str()
            )),
            expires_at: created.expires_at,
            max_views,
        })
    }

    /// Copies a new link. It is treated as a secret: kept out of clipboard
    /// history and cleared after the usual time.
    pub fn copy_link(&self, link: &NewLink) -> Result<Copied, AppError> {
        self.copy_text("Link", &link.url, true)
    }

    /// Every link this account made, newest first, with the titles of the
    /// items that are still on this computer (D-33).
    pub async fn links(&self) -> Result<Vec<LinkRow>, AppError> {
        let list = self.api().links().await?;
        Ok(list
            .links
            .into_iter()
            .map(|info| {
                let entry = self.entry(info.item_id);
                LinkRow {
                    title: entry.as_ref().map(|entry| entry.title.clone()),
                    kind: entry.map(|entry| entry.kind),
                    info,
                }
            })
            .collect())
    }

    /// Stops a link at once (D-33 Revoke).
    pub async fn revoke_link(&self, id: Uuid) -> Result<(), AppError> {
        Ok(self.api().revoke_link(id).await?)
    }
}
