//! The in-memory search index: one entry per item, built from decrypted
//! titles, usernames, websites and notes when the vault unlocks, and dropped
//! when it locks. Passwords, card numbers and other secrets are never in it.

use std::collections::HashMap;

use kandado_model::{ItemContent, ItemData, ItemKind};
use uuid::Uuid;

/// What a list row and a search result need to know about an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: Uuid,
    pub vault_id: Uuid,
    pub kind: ItemKind,
    pub title: String,
    /// The most useful second fact: the username, the card's last four digits,
    /// the first line of a note.
    pub subtitle: String,
    pub favourite: bool,
    /// Unix seconds.
    pub created_at: i64,
    pub updated_at: i64,
    pub trashed_at: Option<i64>,
    /// Whether there is a password to copy from the row.
    pub has_password: bool,
    /// Lower-cased text the search looks in.
    haystack: String,
    title_lower: String,
}

fn last_four(number: &str) -> String {
    let digits: Vec<char> = number.chars().filter(char::is_ascii_digit).collect();
    if digits.len() < 4 {
        return String::new();
    }
    let tail: String = digits[digits.len() - 4..].iter().collect();
    format!("•••• {tail}")
}

fn subtitle(content: &ItemContent) -> String {
    match &content.data {
        ItemData::Login(login) => {
            if login.username.is_empty() {
                login
                    .websites
                    .first()
                    .map(|site| site.url.clone())
                    .unwrap_or_default()
            } else {
                login.username.clone()
            }
        }
        ItemData::Alias(alias) => alias.address.clone(),
        ItemData::Card(card) => last_four(card.number.expose()),
        ItemData::Note(_) => content
            .notes
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default()
            .trim()
            .to_owned(),
        ItemData::Identity(identity) => {
            let name = [&identity.first_name, &identity.last_name]
                .into_iter()
                .filter(|part| !part.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            if name.is_empty() {
                identity.email.clone()
            } else {
                name
            }
        }
        ItemData::Password(_) => "Generated password".to_owned(),
        ItemData::Passkey(passkey) => passkey.rp_id.clone(),
    }
}

impl Entry {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: Uuid,
        vault_id: Uuid,
        content: &ItemContent,
        created_at: i64,
        updated_at: i64,
        trashed_at: Option<i64>,
    ) -> Self {
        let mut haystack = vec![content.title.as_str(), content.notes.as_str()];
        let mut has_password = false;
        match &content.data {
            ItemData::Login(login) => {
                haystack.push(&login.username);
                haystack.extend(login.websites.iter().map(|site| site.url.as_str()));
                has_password = !login.password.is_empty();
            }
            ItemData::Alias(alias) => haystack.push(&alias.address),
            ItemData::Card(card) => haystack.push(&card.cardholder),
            ItemData::Note(_) => {}
            ItemData::Identity(identity) => haystack.extend([
                identity.first_name.as_str(),
                &identity.last_name,
                &identity.email,
                &identity.phone,
                &identity.company,
            ]),
            ItemData::Password(password) => has_password = !password.password.is_empty(),
            ItemData::Passkey(passkey) => {
                haystack.extend([passkey.rp_id.as_str(), &passkey.user_name]);
            }
        }
        // Custom field labels are searchable; their values are secrets.
        haystack.extend(
            content
                .custom_fields
                .iter()
                .map(|field| field.label.as_str()),
        );
        Self {
            id,
            vault_id,
            kind: content.kind(),
            title: content.title.clone(),
            subtitle: subtitle(content),
            favourite: content.favourite,
            created_at,
            updated_at,
            trashed_at,
            has_password,
            haystack: haystack.join("\n").to_lowercase(),
            title_lower: content.title.to_lowercase(),
        }
    }

    pub fn is_trashed(&self) -> bool {
        self.trashed_at.is_some()
    }

    /// How well this entry answers `words` (already lower-cased): `None` if
    /// any word is missing, otherwise lower is better.
    fn rank(&self, words: &[&str]) -> Option<u8> {
        if !words.iter().all(|word| self.haystack.contains(word)) {
            return None;
        }
        let first = words.first()?;
        Some(if self.title_lower.starts_with(first) {
            0
        } else if words.iter().all(|word| self.title_lower.contains(word)) {
            1
        } else {
            2
        })
    }
}

/// Which items a list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Filter {
    /// One vault, or all of them.
    pub vault: Option<Uuid>,
    pub kind: Option<ItemKind>,
    /// The trash instead of the vaults.
    pub trash: bool,
}

#[derive(Default)]
pub struct Index {
    entries: HashMap<Uuid, Entry>,
}

fn by_title(a: &&Entry, b: &&Entry) -> std::cmp::Ordering {
    a.title_lower
        .cmp(&b.title_lower)
        .then_with(|| a.id.cmp(&b.id))
}

impl Index {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, id: Uuid) -> Option<&Entry> {
        self.entries.get(&id)
    }

    pub(crate) fn upsert(&mut self, entry: Entry) {
        self.entries.insert(entry.id, entry);
    }

    pub(crate) fn remove(&mut self, id: Uuid) {
        self.entries.remove(&id);
    }

    pub(crate) fn get_mut(&mut self, id: Uuid) -> Option<&mut Entry> {
        self.entries.get_mut(&id)
    }

    /// The items a list shows. Vault lists are ordered by title; the trash
    /// shows what was deleted last first.
    pub fn list(&self, filter: Filter) -> Vec<&Entry> {
        let mut entries: Vec<&Entry> = self
            .entries
            .values()
            .filter(|entry| entry.is_trashed() == filter.trash)
            .filter(|entry| filter.vault.is_none_or(|vault| entry.vault_id == vault))
            .filter(|entry| filter.kind.is_none_or(|kind| entry.kind == kind))
            .collect();
        if filter.trash {
            entries.sort_by(|a, b| b.trashed_at.cmp(&a.trashed_at).then_with(|| by_title(a, b)));
        } else {
            entries.sort_by(by_title);
        }
        entries
    }

    /// How many items are in a vault (or in all of them), not counting the trash.
    pub fn count(&self, vault: Option<Uuid>) -> usize {
        self.entries
            .values()
            .filter(|entry| !entry.is_trashed())
            .filter(|entry| vault.is_none_or(|vault| entry.vault_id == vault))
            .count()
    }

    pub fn trash_count(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.is_trashed())
            .count()
    }

    /// Searches every vault. Every word of the query must appear somewhere in
    /// the item; items whose title matches come first. The trash is left out.
    pub fn search(&self, query: &str) -> Vec<&Entry> {
        let query = query.to_lowercase();
        let words: Vec<&str> = query.split_whitespace().collect();
        if words.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<(u8, &Entry)> = self
            .entries
            .values()
            .filter(|entry| !entry.is_trashed())
            .filter_map(|entry| Some((entry.rank(&words)?, entry)))
            .collect();
        hits.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| by_title(&a.1, &b.1)));
        hits.into_iter().map(|(_, entry)| entry).collect()
    }
}

#[cfg(test)]
mod tests {
    use kandado_model::item::{Card, Login, Website};

    use super::*;

    fn login(title: &str, username: &str, url: &str, password: &str) -> ItemContent {
        ItemContent::new(
            title,
            ItemData::Login(Login {
                username: username.into(),
                password: password.into(),
                websites: vec![Website {
                    url: url.into(),
                    ..Website::default()
                }],
                ..Login::default()
            }),
        )
    }

    fn index() -> Index {
        let vault = Uuid::from_u128(100);
        let mut index = Index::default();
        let items = [
            login("GitHub", "juan-dc", "https://github.com", "pw-one"),
            login("GitHub (work)", "jdelacruz-acme", "https://github.com", ""),
            login("Shopee", "juan@example.com", "https://shopee.ph", "pw-two"),
            ItemContent {
                title: "Recovery codes".into(),
                notes: "\nGitHub: abcd-efgh".into(),
                ..ItemContent::default()
            },
            ItemContent::new(
                "BPI Visa",
                ItemData::Card(Card {
                    cardholder: "Juan Dela Cruz".into(),
                    number: "4821 0094 3310 4821".into(),
                    ..Card::default()
                }),
            ),
        ];
        for (n, content) in items.iter().enumerate() {
            index.upsert(Entry::new(
                Uuid::from_u128(n as u128 + 1),
                vault,
                content,
                10,
                20,
                None,
            ));
        }
        index
    }

    fn titles(entries: Vec<&Entry>) -> Vec<&str> {
        entries.iter().map(|entry| entry.title.as_str()).collect()
    }

    #[test]
    fn subtitles_show_the_useful_second_fact() {
        let index = index();
        assert_eq!(index.get(Uuid::from_u128(1)).unwrap().subtitle, "juan-dc");
        assert_eq!(
            index.get(Uuid::from_u128(4)).unwrap().subtitle,
            "GitHub: abcd-efgh"
        );
        assert_eq!(index.get(Uuid::from_u128(5)).unwrap().subtitle, "•••• 4821");
    }

    #[test]
    fn search_puts_title_matches_first_and_looks_in_notes_and_websites() {
        let index = index();
        assert_eq!(
            titles(index.search("git")),
            ["GitHub", "GitHub (work)", "Recovery codes"]
        );
        assert_eq!(titles(index.search("SHOPEE.PH")), ["Shopee"]);
        assert_eq!(titles(index.search("github acme")), ["GitHub (work)"]);
        assert!(index.search("   ").is_empty());
        assert!(index.search("nothing-here").is_empty());
    }

    #[test]
    fn secrets_are_not_searchable() {
        let index = index();
        assert!(index.search("pw-one").is_empty());
        assert!(index.search("4821 0094").is_empty());
        assert!(index.get(Uuid::from_u128(1)).unwrap().has_password);
        assert!(!index.get(Uuid::from_u128(2)).unwrap().has_password);
    }

    #[test]
    fn lists_filter_by_vault_kind_and_trash() {
        let mut index = index();
        index.get_mut(Uuid::from_u128(3)).unwrap().trashed_at = Some(99);
        let all = Filter::default();
        assert_eq!(index.list(all).len(), 4);
        assert_eq!(index.count(None), 4);
        assert_eq!(index.trash_count(), 1);
        assert_eq!(
            titles(index.list(Filter { trash: true, ..all })),
            ["Shopee"]
        );
        assert_eq!(
            titles(index.list(Filter {
                kind: Some(ItemKind::Card),
                ..all
            })),
            ["BPI Visa"]
        );
        assert!(
            index
                .list(Filter {
                    vault: Some(Uuid::from_u128(7)),
                    ..all
                })
                .is_empty()
        );
        // Trashed items are not found by search.
        assert!(index.search("shopee").is_empty());
    }
}
