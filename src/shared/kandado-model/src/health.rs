//! Monitor (D-35 to D-39, E-15): what is wrong with the passwords in a vault.
//!
//! Everything here runs on the device over decrypted items, so the same rules
//! hold on the desktop and in the extension:
//!
//! - **Weak**: the strength score is 0 or 1 (the meter's "Weak").
//! - **Reused**: the same password on two or more items.
//! - **Exposed**: the password is in Have I Been Pwned's Pwned Passwords. Only
//!   the first five characters of its SHA-1 hash leave the device
//!   ([`PwnedQuery`]); the answer lists every leaked hash with that prefix and
//!   is searched here.
//! - **Breached site**: a login for a site in Have I Been Pwned's public
//!   breach list, whose password was set before the breach, and which the
//!   person has not marked resolved.
//! - **Could add two-factor**: a login with no two-factor code, for a site the
//!   2FA Directory says offers codes, unless the person said "not for this item".

use std::collections::{HashMap, HashSet};

use sha1::{Digest, Sha1};

use crate::{item::Login, strength, url_match};

/// The highest strength score counted as weak.
pub const WEAK_SCORE: u8 = 1;

/// Whether a password is weak. `user_inputs` are the username and the title,
/// which an attacker would try first.
pub fn is_weak(password: &str, user_inputs: &[&str]) -> Option<strength::Strength> {
    if password.is_empty() {
        return None;
    }
    let strength = strength::estimate(password, user_inputs);
    (strength.score <= WEAK_SCORE).then_some(strength)
}

/// Groups of keys whose passwords are the same, each group of two or more,
/// largest first. Empty passwords are not compared. Within a group the keys
/// keep the order they came in.
pub fn reused_groups<K: Clone>(items: impl IntoIterator<Item = (K, String)>) -> Vec<Vec<K>> {
    let mut by_password: HashMap<String, Vec<K>> = HashMap::new();
    let mut order = Vec::new();
    for (key, password) in items {
        if password.is_empty() {
            continue;
        }
        let entry = by_password.entry(password).or_insert_with_key(|password| {
            order.push(password.clone());
            Vec::new()
        });
        entry.push(key);
    }
    let mut groups: Vec<Vec<K>> = order
        .into_iter()
        .filter_map(|password| by_password.remove(&password))
        .filter(|group| group.len() > 1)
        .collect();
    groups.sort_by_key(|group| std::cmp::Reverse(group.len()));
    groups
}

/// A password's SHA-1, split the way the Pwned Passwords range API wants it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PwnedQuery {
    /// The first five hex characters, upper case: sent to the server.
    pub prefix: String,
    /// The other thirty-five: never sent, looked up in the answer.
    pub suffix: String,
}

impl PwnedQuery {
    pub fn of(password: &str) -> Self {
        let hash: String = Sha1::digest(password.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect();
        let (prefix, suffix) = hash.split_at(5);
        Self {
            prefix: prefix.to_owned(),
            suffix: suffix.to_owned(),
        }
    }

    /// How many times this password was seen in breaches, from a range answer
    /// (`SUFFIX:COUNT` lines). Padding lines have a count of 0 and never match.
    pub fn times_seen(&self, answer: &str) -> u64 {
        answer
            .lines()
            .filter_map(|line| line.trim().split_once(':'))
            .find(|(suffix, _)| suffix.eq_ignore_ascii_case(&self.suffix))
            .and_then(|(_, count)| count.trim().parse().ok())
            .unwrap_or(0)
    }
}

/// A breached site, as Have I Been Pwned lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreachedSite {
    /// HIBP's stable name, such as `Adobe`.
    pub name: String,
    /// The site's domain, such as `adobe.com`.
    pub domain: String,
    /// When the breach happened, Unix seconds (midnight UTC of its date).
    pub breach_date: i64,
}

/// The day a `YYYY-MM-DD` date starts, in Unix seconds (UTC).
pub fn date_to_unix(date: &str) -> Option<i64> {
    let mut parts = date.trim().splitn(3, '-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.get(..2)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Days from 1970-01-01 to the date (Howard Hinnant's days_from_civil).
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some((era * 146_097 + day_of_era - 719_468) * 86_400)
}

/// The registrable domains of a login's websites.
pub fn login_domains(login: &Login) -> HashSet<String> {
    login
        .websites
        .iter()
        .filter_map(|site| url_match::registrable_domain(&site.url))
        .collect()
}

/// The breaches that apply to a login: a breach of one of its sites after its
/// password was set, not marked resolved. `password_set` is
/// [`Login::password_changed_at`], or the item's creation when that is absent.
pub fn breaches_for<'a>(
    login: &Login,
    password_set: i64,
    breaches: &'a [BreachedSite],
) -> Vec<&'a BreachedSite> {
    if login.password.is_empty() {
        return Vec::new();
    }
    let domains = login_domains(login);
    breaches
        .iter()
        .filter(|breach| {
            url_match::registrable_domain(&breach.domain).is_some_and(|d| domains.contains(&d))
                && breach.breach_date >= password_set
                && !login.resolved_breaches.contains(&breach.name)
        })
        .collect()
}

/// Whether a login could add two-factor: it has none, the person did not say
/// no, and one of its sites is in `sites` (registrable domains from the 2FA
/// Directory).
pub fn could_add_two_factor(login: &Login, sites: &HashSet<String>) -> bool {
    !login.no_two_factor
        && login.totp.as_ref().is_none_or(|totp| totp.is_empty())
        && login_domains(login)
            .iter()
            .any(|domain| sites.contains(domain))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Website;

    fn login(password: &str, url: &str) -> Login {
        Login {
            username: "juan".into(),
            password: password.into(),
            websites: vec![Website {
                url: url.into(),
                ..Website::default()
            }],
            ..Login::default()
        }
    }

    #[test]
    fn weak_is_the_meters_weak() {
        assert!(is_weak("password1", &[]).is_some());
        assert!(is_weak("juan2024", &["juan"]).is_some());
        assert!(is_weak("correct-Horse-battery-staple-42", &[]).is_none());
        assert!(
            is_weak("", &[]).is_none(),
            "an empty password is not checked"
        );
    }

    #[test]
    fn reuse_is_grouped_and_ordered() {
        let groups = reused_groups(vec![
            (1, "a".to_owned()),
            (2, "b".to_owned()),
            (3, "a".to_owned()),
            (4, String::new()),
            (5, String::new()),
            (6, "b".to_owned()),
            (7, "b".to_owned()),
            (8, "c".to_owned()),
        ]);
        assert_eq!(groups, vec![vec![2, 6, 7], vec![1, 3]]);
    }

    #[test]
    fn only_the_prefix_is_sent_and_padding_never_matches() {
        // SHA-1("password") = 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8
        let query = PwnedQuery::of("password");
        assert_eq!(query.prefix, "5BAA6");
        assert_eq!(query.suffix, "1E4C9B93F3F0682250B6CF8331B7EE68FD8");
        let answer = "0018A45C4D1DEF81644B54AB7F969B88D65:1\r\n\
                      1E4C9B93F3F0682250B6CF8331B7EE68FD8:9659365\r\n\
                      FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF:0\r\n";
        assert_eq!(query.times_seen(answer), 9_659_365);
        assert_eq!(PwnedQuery::of("something else").times_seen(answer), 0);
        let padded = PwnedQuery {
            prefix: "5BAA6".into(),
            suffix: "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF".into(),
        };
        assert_eq!(padded.times_seen(answer), 0);
    }

    #[test]
    fn dates_are_utc_midnights() {
        assert_eq!(date_to_unix("1970-01-01"), Some(0));
        assert_eq!(date_to_unix("2000-03-01"), Some(951_868_800));
        assert_eq!(date_to_unix("2026-11-02"), Some(1_793_577_600));
        assert_eq!(date_to_unix("2026-11-02T00:00:00Z"), Some(1_793_577_600));
        assert_eq!(date_to_unix("2026-13-01"), None);
        assert_eq!(date_to_unix("soon"), None);
    }

    #[test]
    fn a_breach_counts_only_after_the_password_was_set_and_until_resolved() {
        let breaches = vec![
            BreachedSite {
                name: "Lakbay".into(),
                domain: "lakbay.ph".into(),
                breach_date: date_to_unix("2026-11-02").unwrap(),
            },
            BreachedSite {
                name: "Other".into(),
                domain: "other.example".into(),
                breach_date: date_to_unix("2026-11-02").unwrap(),
            },
        ];
        let before = date_to_unix("2026-01-01").unwrap();
        let after = date_to_unix("2026-11-10").unwrap();
        // Any subdomain of the breached site.
        let mut item = login("pw", "https://accounts.lakbay.ph/login");
        let found = breaches_for(&item, before, &breaches);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Lakbay");
        // Changed since: fine.
        assert!(breaches_for(&item, after, &breaches).is_empty());
        // Marked resolved: fine.
        item.resolved_breaches.push("Lakbay".into());
        assert!(breaches_for(&item, before, &breaches).is_empty());
        // No password, nothing to change.
        assert!(breaches_for(&login("", "lakbay.ph"), before, &breaches).is_empty());
        // Not a lookalike.
        assert!(breaches_for(&login("pw", "notlakbay.ph"), before, &breaches).is_empty());
    }

    #[test]
    fn two_factor_is_offered_for_listed_sites_without_a_code() {
        let sites: HashSet<String> = ["github.com".to_owned()].into();
        let mut item = login("pw", "https://github.com/login");
        assert!(could_add_two_factor(&item, &sites));
        assert!(!could_add_two_factor(&login("pw", "example.com"), &sites));
        item.no_two_factor = true;
        assert!(!could_add_two_factor(&item, &sites));
        item.no_two_factor = false;
        item.totp = Some("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP".into());
        assert!(!could_add_two_factor(&item, &sites));
    }

    #[test]
    fn the_new_login_fields_are_left_out_when_empty() {
        let json = serde_json::to_string(&login("pw", "a.example")).unwrap();
        assert!(!json.contains("password_changed_at"));
        assert!(!json.contains("resolved_breaches"));
        assert!(!json.contains("no_two_factor"));
        let mut item = login("pw", "a.example");
        item.password_changed_at = Some(5);
        item.resolved_breaches.push("X".into());
        item.no_two_factor = true;
        let json = serde_json::to_string(&item).unwrap();
        let back: Login = serde_json::from_str(&json).unwrap();
        assert_eq!(back, item);
    }
}
