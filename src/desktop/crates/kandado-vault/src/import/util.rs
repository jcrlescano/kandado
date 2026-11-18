//! What the importers share: reading JSON values, building items, and the
//! rule for what is worth importing.

use kandado_model::{
    ItemContent, ItemData, Secret,
    item::{CustomField, Login, Website},
    totp::Totp,
    url_match::MatchMode,
};
use serde_json::Value;

/// A string field of a JSON object, or "" if it is missing or not a string.
pub(super) fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

pub(super) fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

pub(super) fn object<'a>(value: &'a Value, key: &str) -> &'a Value {
    static NULL: Value = Value::Null;
    value.get(key).unwrap_or(&NULL)
}

/// "extraAddressDetails" becomes "Extra address details".
pub(super) fn humanize(key: &str) -> String {
    let mut out = String::new();
    for (index, c) in key.chars().enumerate() {
        if c == '_' || c == '-' {
            out.push(' ');
        } else if c.is_uppercase() && index > 0 {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else if index == 0 {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
    }
    out
}

pub(super) fn custom(
    label: impl Into<String>,
    value: impl Into<String>,
    hidden: bool,
) -> CustomField {
    CustomField {
        label: label.into(),
        value: Secret::new(value.into()),
        hidden,
        ..CustomField::default()
    }
}

/// A website entry for a saved address, trimmed.
pub(super) fn website(url: &str, mode: MatchMode) -> Option<Website> {
    let url = url.trim();
    (!url.is_empty()).then(|| Website {
        url: url.to_owned(),
        match_mode: mode,
        ..Website::default()
    })
}

/// A two-factor setup key, if what the source holds is one Kandado can use.
/// Anything else is returned as `Err` with the text, to be kept as a field.
pub(super) fn totp(raw: &str) -> Result<Secret, String> {
    let raw = raw.trim();
    match Totp::parse(raw) {
        Ok(parsed) => Ok(Secret::new(parsed.to_uri().as_str())),
        Err(_) => Err(raw.to_owned()),
    }
}

/// The title to use when an item has none: the site, then the username.
pub(super) fn fallback_title(title: &str, urls: &[String], username: &str) -> String {
    let title = title.trim();
    if !title.is_empty() {
        return title.to_owned();
    }
    for url in urls {
        let host = url
            .trim()
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split(['/', '?', '#'])
            .next()
            .unwrap_or("");
        if !host.is_empty() {
            return host.to_owned();
        }
    }
    username.trim().to_owned()
}

/// A login, with the two-factor key kept as a field if it cannot be read.
pub(super) struct LoginParts<'a> {
    pub title: &'a str,
    pub username: &'a str,
    pub password: &'a str,
    pub urls: Vec<String>,
    pub totp: &'a str,
    pub notes: &'a str,
    pub favourite: bool,
    pub custom_fields: Vec<CustomField>,
}

impl LoginParts<'_> {
    /// Nothing here is worth an item: no title, no site, no sign-in, no note.
    pub(super) fn is_empty(&self) -> bool {
        self.title.trim().is_empty()
            && self.urls.iter().all(|url| url.trim().is_empty())
            && self.username.trim().is_empty()
            && self.password.is_empty()
            && self.notes.trim().is_empty()
            && self.totp.trim().is_empty()
            && self.custom_fields.is_empty()
    }

    pub(super) fn build(mut self) -> ItemContent {
        let title = fallback_title(self.title, &self.urls, self.username);
        let totp = if self.totp.trim().is_empty() {
            None
        } else {
            match totp(self.totp) {
                Ok(setup) => Some(setup),
                Err(text) => {
                    self.custom_fields
                        .push(custom("Two-factor setup key", text, true));
                    None
                }
            }
        };
        ItemContent {
            title,
            notes: self.notes.to_owned(),
            favourite: self.favourite,
            custom_fields: self.custom_fields,
            data: ItemData::Login(Login {
                username: self.username.trim().to_owned(),
                password: Secret::new(self.password),
                websites: self
                    .urls
                    .iter()
                    .filter_map(|url| website(url, MatchMode::Domain))
                    .collect(),
                totp,
                ..Login::default()
            }),
            ..ItemContent::default()
        }
    }
}

/// A note item.
pub(super) fn note(
    title: &str,
    text: &str,
    custom_fields: Vec<CustomField>,
    favourite: bool,
) -> ItemContent {
    ItemContent {
        title: title.trim().to_owned(),
        notes: text.to_owned(),
        favourite,
        custom_fields,
        ..ItemContent::default()
    }
}

/// "08/2029", "2029-08", "8/29", "082029" and "202908" as a month and a year.
pub(super) fn month_year(text: &str) -> Option<kandado_model::item::Expiry> {
    let text = text.trim();
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    let (month, year) = if text.contains('/') || text.contains('-') {
        let mut parts = text.split(['/', '-']);
        let (a, b) = (parts.next()?.trim(), parts.next()?.trim());
        if a.len() == 4 { (b, a) } else { (a, b) }
    } else if digits.len() == 6 {
        // 202908 or 082029
        if digits.starts_with("20") && !digits.starts_with("200") {
            (&digits[4..], &digits[..4])
        } else {
            (&digits[..2], &digits[2..])
        }
    } else {
        return None;
    };
    let month: u8 = month.parse().ok()?;
    let mut year: u16 = year.parse().ok()?;
    if year < 100 {
        year += 2000;
    }
    ((1..=12).contains(&month) && (2000..=2999).contains(&year))
        .then_some(kandado_model::item::Expiry { month, year })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_forms() {
        let aug = Some(kandado_model::item::Expiry {
            month: 8,
            year: 2029,
        });
        for text in ["08/2029", "2029-08", "8/29", "08/29", "082029", "202908"] {
            assert_eq!(month_year(text), aug, "{text}");
        }
        for text in ["", "13/2029", "soon", "2029"] {
            assert_eq!(month_year(text), None, "{text}");
        }
    }

    #[test]
    fn names_are_made_readable() {
        assert_eq!(humanize("extraAddressDetails"), "Extra address details");
        assert_eq!(humanize("social_security"), "Social security");
    }

    #[test]
    fn titles_fall_back_to_the_site_then_the_username() {
        assert_eq!(
            fallback_title("", &["https://github.com/login".into()], "juan"),
            "github.com"
        );
        assert_eq!(fallback_title("  ", &[], "juan"), "juan");
        assert_eq!(fallback_title("GitHub", &[], ""), "GitHub");
    }

    #[test]
    fn an_unreadable_two_factor_key_is_kept_as_a_field() {
        let content = LoginParts {
            title: "Site",
            username: "u",
            password: "p",
            urls: vec![],
            totp: "not a key!",
            notes: "",
            favourite: false,
            custom_fields: vec![],
        }
        .build();
        assert_eq!(content.custom_fields.len(), 1);
        assert_eq!(content.custom_fields[0].label, "Two-factor setup key");
        assert!(content.custom_fields[0].hidden);
    }
}
