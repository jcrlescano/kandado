//! Monitor, called Health in the app (D-35 to D-39): what to fix in the
//! vault, worked out on this computer with the rules in
//! `kandado_model::health`.
//!
//! The server is asked for three public things only: the Pwned Passwords
//! answer for the first five hex characters of each password's SHA-1, Have I
//! Been Pwned's breached sites, and the 2FA Directory's sites with two-factor.
//! Which sites and passwords are in the vault never leaves the computer. When
//! the server cannot be reached, the local checks (weak, reused) still run
//! and the result says what could not be checked.

use std::collections::{HashMap, HashSet};

use kandado_model::{
    ItemData, ItemKind,
    health::{self, BreachedSite, PwnedQuery},
};
use kandado_protocol::BreachSite;
use kandado_vault::index::Filter;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{AppError, KandadoApp, vault::now};

/// An item as the Health lists show it. Holds no secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthItem {
    pub id: Uuid,
    pub vault_id: Uuid,
    pub kind: ItemKind,
    pub title: String,
    /// The username, when there is one.
    pub subtitle: String,
}

/// A breached site and the logins it affects (D-36).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreachGroup {
    /// Have I Been Pwned's name: what "Mark as resolved" records.
    pub name: String,
    pub title: String,
    pub domain: String,
    /// Unix seconds.
    pub breach_date: i64,
    /// What was exposed, as Have I Been Pwned words it.
    pub data_classes: Vec<String>,
    pub items: Vec<HealthItem>,
}

/// A password found in Pwned Passwords.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exposed {
    pub item: HealthItem,
    pub times_seen: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Weak {
    pub item: HealthItem,
    /// 0 or 1.
    pub score: u8,
    pub bits: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwoFactor {
    pub item: HealthItem,
    /// Where the site explains how to turn it on.
    pub documentation: Option<String>,
}

/// A source the check could not reach this time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unchecked {
    /// Exposed passwords (Pwned Passwords).
    Passwords,
    /// Breached sites.
    Breaches,
    /// Sites with two-factor.
    TwoFactor,
}

/// One Health check.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Health {
    /// Unix seconds.
    pub checked_at: i64,
    /// Logins and passwords looked at.
    pub items_checked: usize,
    /// Breaches still open, newest first.
    pub breaches: Vec<BreachGroup>,
    /// Breaches marked resolved on every login they affect.
    pub resolved: Vec<BreachGroup>,
    /// Most seen first.
    pub exposed: Vec<Exposed>,
    /// Largest group first.
    pub reused: Vec<Vec<HealthItem>>,
    pub weak: Vec<Weak>,
    pub two_factor: Vec<TwoFactor>,
    pub unchecked: Vec<Unchecked>,
}

impl Health {
    /// Items with something to fix (breach, exposed, reused, weak), each once.
    /// Two-factor is a suggestion, not a fault.
    pub fn to_fix(&self) -> usize {
        let mut ids = HashSet::new();
        ids.extend(
            self.breaches
                .iter()
                .flat_map(|group| group.items.iter().map(|i| i.id)),
        );
        ids.extend(self.exposed.iter().map(|exposed| exposed.item.id));
        ids.extend(self.reused.iter().flatten().map(|item| item.id));
        ids.extend(self.weak.iter().map(|weak| weak.item.id));
        ids.len()
    }

    /// The breaches tile and list count both breached sites and exposed passwords.
    pub fn breach_count(&self) -> usize {
        self.breaches.len() + usize::from(!self.exposed.is_empty())
    }
}

/// What the check needs of one item. Secrets are wiped when it is dropped.
struct Checked {
    item: HealthItem,
    password: Zeroizing<String>,
    /// Logins only.
    login: Option<kandado_model::item::Login>,
    created_at: i64,
}

fn digest(password: &str) -> String {
    Sha256::digest(password.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn breached_sites(list: &[BreachSite]) -> Vec<(BreachedSite, &BreachSite)> {
    list.iter()
        .filter_map(|site| {
            Some((
                BreachedSite {
                    name: site.name.clone(),
                    domain: site.domain.clone(),
                    breach_date: health::date_to_unix(&site.breach_date)?,
                },
                site,
            ))
        })
        .collect()
}

impl KandadoApp {
    /// Every login and saved password in the vaults (not the trash), decrypted.
    fn health_items(&self) -> Result<Vec<Checked>, AppError> {
        let entries = self.entries(Filter {
            vault: None,
            kind: None,
            trash: false,
        });
        let mut out = Vec::new();
        for entry in entries {
            if !matches!(entry.kind, ItemKind::Login | ItemKind::Password) {
                continue;
            }
            let item = self.item(entry.id)?;
            let (password, login) = match item.content.data {
                ItemData::Login(login) => (
                    Zeroizing::new(login.password.expose().to_owned()),
                    Some(login),
                ),
                ItemData::Password(saved) => {
                    (Zeroizing::new(saved.password.expose().to_owned()), None)
                }
                _ => continue,
            };
            out.push(Checked {
                item: HealthItem {
                    id: entry.id,
                    vault_id: entry.vault_id,
                    kind: entry.kind,
                    title: item.content.title.clone(),
                    subtitle: login
                        .as_ref()
                        .map(|l| l.username.clone())
                        .unwrap_or_default(),
                },
                password,
                login,
                created_at: item.created_at,
            });
        }
        Ok(out)
    }

    /// Checks the whole vault (D-35). Weak and reused are worked out here;
    /// exposed passwords, breached sites and two-factor need the server, and
    /// are listed in [`Health::unchecked`] when it cannot be reached.
    pub async fn check_health(&self) -> Result<Health, AppError> {
        let items = self.health_items()?;
        let mut health = Health {
            checked_at: now(),
            items_checked: items.len(),
            ..Health::default()
        };

        for checked in &items {
            let username = checked.login.as_ref().map_or("", |l| l.username.as_str());
            if let Some(strength) =
                health::is_weak(&checked.password, &[username, &checked.item.title])
            {
                health.weak.push(Weak {
                    item: checked.item.clone(),
                    score: strength.score,
                    bits: strength.bits,
                });
            }
        }
        // Compared by digest, so no password is copied into the grouping.
        let by_id: HashMap<Uuid, &HealthItem> =
            items.iter().map(|c| (c.item.id, &c.item)).collect();
        health.reused = health::reused_groups(
            items
                .iter()
                .filter(|c| !c.password.is_empty())
                .map(|c| (c.item.id, digest(&c.password))),
        )
        .into_iter()
        .map(|group| group.iter().map(|id| by_id[id].clone()).collect())
        .collect();

        // Exposed passwords: one question per distinct prefix.
        let queries: Vec<(usize, PwnedQuery)> = items
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.password.is_empty())
            .map(|(index, c)| (index, PwnedQuery::of(&c.password)))
            .collect();
        let mut answers: HashMap<String, String> = HashMap::new();
        let mut reached = true;
        for (_, query) in &queries {
            if answers.contains_key(&query.prefix) {
                continue;
            }
            match self.api().pwned_range(&query.prefix).await {
                Ok(answer) => {
                    answers.insert(query.prefix.clone(), answer);
                }
                Err(_) => {
                    reached = false;
                    break;
                }
            }
        }
        if reached {
            for (index, query) in &queries {
                let seen = query.times_seen(&answers[&query.prefix]);
                if seen > 0 {
                    health.exposed.push(Exposed {
                        item: items[*index].item.clone(),
                        times_seen: seen,
                    });
                }
            }
            health
                .exposed
                .sort_by_key(|e| std::cmp::Reverse(e.times_seen));
        } else {
            health.unchecked.push(Unchecked::Passwords);
        }

        // Breached sites.
        match self.api().breach_list().await {
            Ok(list) => {
                let sites = breached_sites(&list.breaches);
                let only: Vec<BreachedSite> = sites.iter().map(|(site, _)| site.clone()).collect();
                let mut open: HashMap<String, BreachGroup> = HashMap::new();
                let mut resolved: HashMap<String, BreachGroup> = HashMap::new();
                for checked in &items {
                    let Some(login) = &checked.login else {
                        continue;
                    };
                    let set = login.password_changed_at.unwrap_or(checked.created_at);
                    // Resolved ones: as if nothing were resolved, then split.
                    let mut unresolved = login.clone();
                    unresolved.resolved_breaches.clear();
                    for site in health::breaches_for(&unresolved, set, &only) {
                        let (_, info) = sites.iter().find(|(s, _)| s.name == site.name).unwrap();
                        let target = if login.resolved_breaches.contains(&site.name) {
                            &mut resolved
                        } else {
                            &mut open
                        };
                        target
                            .entry(site.name.clone())
                            .or_insert_with(|| BreachGroup {
                                name: site.name.clone(),
                                title: info.title.clone(),
                                domain: info.domain.clone(),
                                breach_date: site.breach_date,
                                data_classes: info.data_classes.clone(),
                                items: Vec::new(),
                            })
                            .items
                            .push(checked.item.clone());
                    }
                }
                health.breaches = open.into_values().collect();
                health
                    .breaches
                    .sort_by_key(|g| std::cmp::Reverse(g.breach_date));
                // Resolved only when no login still has it open.
                health.resolved = resolved
                    .into_values()
                    .filter(|g| !health.breaches.iter().any(|open| open.name == g.name))
                    .collect();
                health
                    .resolved
                    .sort_by_key(|g| std::cmp::Reverse(g.breach_date));
            }
            Err(_) => health.unchecked.push(Unchecked::Breaches),
        }

        // Sites with two-factor.
        match self.api().two_factor_sites().await {
            Ok(list) => {
                let docs: HashMap<String, Option<String>> = list
                    .sites
                    .into_iter()
                    .filter_map(|site| {
                        let domain = kandado_model::url_match::registrable_domain(&site.domain)?;
                        Some((domain, site.documentation))
                    })
                    .collect();
                let domains: HashSet<String> = docs.keys().cloned().collect();
                for checked in &items {
                    let Some(login) = &checked.login else {
                        continue;
                    };
                    if health::could_add_two_factor(login, &domains) {
                        let documentation = health::login_domains(login)
                            .iter()
                            .find_map(|d| docs.get(d).cloned().flatten());
                        health.two_factor.push(TwoFactor {
                            item: checked.item.clone(),
                            documentation,
                        });
                    }
                }
            }
            Err(_) => health.unchecked.push(Unchecked::TwoFactor),
        }
        Ok(health)
    }

    /// D-36 "Mark as resolved": this login stops counting this breach.
    pub fn resolve_breach(&self, item: Uuid, breach: &str) -> Result<(), AppError> {
        let current = self.item(item)?;
        let mut content = current.content.clone();
        let ItemData::Login(login) = &mut content.data else {
            return Err(AppError::WrongState);
        };
        if !login.resolved_breaches.iter().any(|name| name == breach) {
            login.resolved_breaches.push(breach.to_owned());
        }
        self.update_item(item, current.vault_id, &content)
    }

    /// D-39 "Not for this item".
    pub fn dismiss_two_factor(&self, item: Uuid) -> Result<(), AppError> {
        let current = self.item(item)?;
        let mut content = current.content.clone();
        let ItemData::Login(login) = &mut content.data else {
            return Err(AppError::WrongState);
        };
        login.no_two_factor = true;
        self.update_item(item, current.vault_id, &content)
    }
}
