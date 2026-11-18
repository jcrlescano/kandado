//! Making the items of a [`Plan`] in the vault: all of them or none.

use kandado_model::VaultMeta;
use uuid::Uuid;

use super::{Plan, Skipped};
use crate::{Session, Store, StoreError, VaultError, items, vaults};

/// Where imported items go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportTarget {
    /// Everything into this vault.
    Into(Uuid),
    /// Each of the source's vaults becomes a vault of the same name (one that
    /// already exists is reused). Items from a source without vaults, such
    /// as a CSV, go into `fallback`.
    BySource { fallback: Uuid },
}

/// How far an import has got, for the progress bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
}

/// What an import did: the sentence under D-47, "210 imported, 4 skipped".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportReport {
    pub imported: usize,
    /// Of those, items that went to the trash.
    pub in_trash: usize,
    /// Items that were in the source's trash and were left out by choice.
    pub trash_left_out: usize,
    /// Email aliases kept as notes.
    pub aliases_as_notes: usize,
    pub vaults_created: usize,
    /// Everything left out because Kandado cannot use it, with the reasons.
    pub skipped: Vec<Skipped>,
    /// What the source has that Kandado does not keep.
    pub ignored: Vec<String>,
}

fn into_store(error: VaultError) -> StoreError {
    match error {
        VaultError::Store(error) => error,
        VaultError::NotFound => StoreError::NotFound,
        VaultError::Cancelled => StoreError::Cancelled,
        other => {
            tracing::warn!(%other, "an import could not be finished");
            StoreError::Corrupt
        }
    }
}

/// Imports a plan. `include_trashed` decides whether items that were in the
/// source's trash come in (to Kandado's trash) or stay out. `progress` is
/// called after each item; if it returns false the import stops and nothing
/// is kept.
pub fn apply(
    store: &Store,
    session: &mut Session,
    plan: &Plan,
    target: ImportTarget,
    include_trashed: bool,
    now: i64,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<ImportReport, VaultError> {
    let wanted: Vec<_> = plan
        .items
        .iter()
        .filter(|item| include_trashed || !item.trashed)
        .collect();
    let total = wanted.len();
    let mut report = ImportReport {
        trash_left_out: plan.items.len() - total,
        skipped: plan.skipped.clone(),
        ignored: plan.ignored.clone(),
        ..ImportReport::default()
    };
    let mut created_vaults: Vec<Uuid> = Vec::new();
    let mut done = 0;
    let result = store.atomically(|store| {
        // Resolve the source's vaults once.
        let mut known: Vec<(String, Uuid)> = vaults::list(store, session)
            .map_err(into_store)?
            .into_iter()
            .map(|info| (info.meta.name.to_lowercase(), info.id))
            .collect();
        for item in &wanted {
            let vault = match (target, item.vault.as_deref()) {
                (ImportTarget::Into(vault), _) => vault,
                (ImportTarget::BySource { fallback }, None) => fallback,
                (ImportTarget::BySource { .. }, Some(name)) => {
                    match known
                        .iter()
                        .find(|(known_name, _)| *known_name == name.to_lowercase())
                    {
                        Some((_, id)) => *id,
                        None => {
                            let meta = plan
                                .styles
                                .iter()
                                .find(|(style_name, _)| style_name == name)
                                .map_or_else(
                                    || VaultMeta::new(name, "teal", "vault"),
                                    |(_, meta)| meta.clone(),
                                );
                            let id =
                                vaults::create(store, session, &meta, now).map_err(into_store)?;
                            created_vaults.push(id);
                            report.vaults_created += 1;
                            known.push((name.to_lowercase(), id));
                            id
                        }
                    }
                }
            };
            let id =
                items::create(store, session, vault, &item.content, now).map_err(into_store)?;
            if item.trashed {
                items::trash(store, session, &[id], now).map_err(into_store)?;
                report.in_trash += 1;
            }
            if item.from_alias {
                report.aliases_as_notes += 1;
            }
            report.imported += 1;
            done += 1;
            if !progress(Progress { done, total }) {
                return Err(StoreError::Cancelled);
            }
        }
        Ok(())
    });
    if let Err(error) = result {
        // The store rolled back; bring the session back in line with it.
        for id in created_vaults {
            session.vault_keys.remove(&id);
        }
        items::load_index(store, session)?;
        return Err(match error {
            StoreError::Cancelled => VaultError::Cancelled,
            other => other.into(),
        });
    }
    Ok(report)
}
