//! Secure links (D-32): one item, as someone without Kandado will see it.
//!
//! The desktop app turns an item into a [`LinkSnapshot`], a flat list of
//! labelled values, and encrypts it with a fresh random link key. The server
//! stores only that ciphertext. The key travels in the link's fragment
//! (`https://…/l/{id}#{key}`), which browsers never send to a server, and the
//! viewer page decrypts in the browser with this same code compiled to
//! WebAssembly.
//!
//! Encoded form: `format version (1 byte) || JSON`, encrypted with
//! XChaCha20-Poly1305 under [`crate::contexts::secure_link`], which binds the
//! blob to the link's id.
//!
//! ```json
//! {"title":"GitHub","kind":"login","notes":"",
//!  "fields":[{"label":"Username","value":"juan","style":"text"}, …]}
//! ```

use data_encoding::BASE64URL_NOPAD;
use kandado_crypto::{KEY_LEN, SecretKey, aead};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{
    Error, ItemContent, ItemData, ItemKind, Result, Secret, contexts,
    item::{Address, Expiry},
};

/// Version of the snapshot encoding described in this module.
pub const FORMAT_VERSION: u8 = 1;

/// How long a link may live, in seconds: 1 hour, 1 day, 7 days, 30 days (D-32).
pub const EXPIRY_CHOICES: [i64; 4] = [3_600, 86_400, 7 * 86_400, 30 * 86_400];

/// How many times a link may be opened: once, 5, 10, or no limit (D-32).
pub const VIEW_CHOICES: [Option<i32>; 4] = [Some(1), Some(5), Some(10), None];

/// What the person who opens the link sees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkSnapshot {
    #[serde(default)]
    pub title: String,
    pub kind: ItemKind,
    #[serde(default)]
    pub fields: Vec<LinkField>,
    #[serde(default)]
    pub notes: String,
}

/// One labelled value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkField {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub value: Secret,
    #[serde(default)]
    pub style: FieldStyle,
}

/// How the viewer shows a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldStyle {
    /// Shown as it is.
    #[default]
    Text,
    /// Masked until the person reveals it.
    Secret,
    /// A web address.
    Url,
    /// A two-factor setup key; the viewer shows the current code.
    Totp,
}

impl LinkField {
    fn new(label: &str, value: impl Into<Secret>, style: FieldStyle) -> Self {
        Self {
            label: label.to_owned(),
            value: value.into(),
            style,
        }
    }
}

impl LinkSnapshot {
    /// The snapshot of an item. Empty values are left out. The two-factor
    /// setup key goes in only with `include_totp`, because it lets the reader
    /// make codes for as long as they keep it. A passkey cannot be shared:
    /// its private key would leave Kandado.
    pub fn of(content: &ItemContent, include_totp: bool) -> Result<Self> {
        let mut fields = Vec::new();
        let mut push = |label: &str, value: &str, style: FieldStyle| {
            if !value.trim().is_empty() {
                fields.push(LinkField::new(label, value, style));
            }
        };
        match &content.data {
            ItemData::Login(login) => {
                push("Username", &login.username, FieldStyle::Text);
                push("Password", login.password.expose(), FieldStyle::Secret);
                for website in &login.websites {
                    push("Website", &website.url, FieldStyle::Url);
                }
                if include_totp && let Some(totp) = &login.totp {
                    push("Two-factor code", totp.expose(), FieldStyle::Totp);
                }
            }
            ItemData::Alias(alias) => push("Address", &alias.address, FieldStyle::Text),
            ItemData::Card(card) => {
                push("Cardholder", &card.cardholder, FieldStyle::Text);
                push("Number", card.number.expose(), FieldStyle::Secret);
                if let Some(expiry) = card.expiry {
                    push("Expires", &expiry_text(expiry), FieldStyle::Text);
                }
                push("Security code", card.cvv.expose(), FieldStyle::Secret);
                push("PIN", card.pin.expose(), FieldStyle::Secret);
            }
            ItemData::Note(_) => {}
            ItemData::Identity(identity) => {
                let name = [
                    identity.first_name.as_str(),
                    identity.middle_name.as_str(),
                    identity.last_name.as_str(),
                ]
                .iter()
                .filter(|part| !part.trim().is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" ");
                push("Name", &name, FieldStyle::Text);
                push("Email", &identity.email, FieldStyle::Text);
                push("Phone", &identity.phone, FieldStyle::Text);
                push("Company", &identity.company, FieldStyle::Text);
                push("Job title", &identity.job_title, FieldStyle::Text);
                push(
                    "Address",
                    &address_text(&identity.address),
                    FieldStyle::Text,
                );
                for document in &identity.documents {
                    let label = if document.label.trim().is_empty() {
                        "Document"
                    } else {
                        document.label.as_str()
                    };
                    push(label, document.number.expose(), FieldStyle::Secret);
                }
            }
            ItemData::Password(password) => {
                push("Password", password.password.expose(), FieldStyle::Secret);
            }
            ItemData::Passkey(_) => return Err(Error::NotShareable),
        }
        for field in &content.custom_fields {
            let label = if field.label.trim().is_empty() {
                "Field"
            } else {
                field.label.as_str()
            };
            let style = if field.hidden {
                FieldStyle::Secret
            } else {
                FieldStyle::Text
            };
            push(label, field.value.expose(), style);
        }
        Ok(Self {
            title: content.title.clone(),
            kind: content.kind(),
            fields,
            notes: content.notes.clone(),
        })
    }

    /// The bytes to encrypt. Wiped from memory when dropped.
    pub fn encode(&self) -> Result<Zeroizing<Vec<u8>>> {
        let mut bytes = Zeroizing::new(vec![FORMAT_VERSION]);
        serde_json::to_writer(&mut *bytes, self).map_err(|_| Error::Format)?;
        Ok(bytes)
    }

    /// Reads bytes made by [`LinkSnapshot::encode`].
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let (&version, json) = bytes.split_first().ok_or(Error::Format)?;
        if version != FORMAT_VERSION {
            return Err(Error::UnsupportedVersion(version));
        }
        serde_json::from_slice(json).map_err(|_| Error::Format)
    }
}

fn expiry_text(expiry: Expiry) -> String {
    format!("{:02}/{}", expiry.month, expiry.year)
}

fn address_text(address: &Address) -> String {
    let city_line = [
        address.postal_code.as_str(),
        address.city.as_str(),
        address.region.as_str(),
    ]
    .iter()
    .filter(|part| !part.trim().is_empty())
    .copied()
    .collect::<Vec<_>>()
    .join(" ");
    [
        address.line1.as_str(),
        address.line2.as_str(),
        city_line.as_str(),
        address.country.as_str(),
    ]
    .iter()
    .filter(|part| !part.trim().is_empty())
    .copied()
    .collect::<Vec<_>>()
    .join(", ")
}

/// Encrypts a snapshot for the link `link_id` with a new random key. Returns
/// the payload for the server and the key for the link's fragment.
pub fn seal(snapshot: &LinkSnapshot, link_id: [u8; 16]) -> Result<(Vec<u8>, SecretKey)> {
    let key = SecretKey::generate().map_err(|_| Error::Random)?;
    let payload = seal_with(&key, snapshot, link_id)?;
    Ok((payload, key))
}

fn seal_with(key: &SecretKey, snapshot: &LinkSnapshot, link_id: [u8; 16]) -> Result<Vec<u8>> {
    aead::encrypt(key, &contexts::secure_link(link_id), &snapshot.encode()?)
        .map_err(|_| Error::Format)
}

/// Decrypts a payload made by [`seal`]. Fails for the wrong key, a payload
/// moved from another link, or any change to the bytes.
pub fn open(key: &SecretKey, link_id: [u8; 16], payload: &[u8]) -> Result<LinkSnapshot> {
    let plain =
        aead::decrypt(key, &contexts::secure_link(link_id), payload).map_err(|_| Error::LinkKey)?;
    LinkSnapshot::decode(&plain)
}

/// The key as it goes in the link's fragment: base64url, no padding, 43 characters.
pub fn key_to_fragment(key: &SecretKey) -> Zeroizing<String> {
    Zeroizing::new(BASE64URL_NOPAD.encode(key.expose()))
}

/// The key from a link's fragment, with or without the leading `#`.
pub fn key_from_fragment(fragment: &str) -> Result<SecretKey> {
    let text = fragment.strip_prefix('#').unwrap_or(fragment);
    let bytes = Zeroizing::new(
        BASE64URL_NOPAD
            .decode(text.as_bytes())
            .map_err(|_| Error::LinkKey)?,
    );
    if bytes.len() != KEY_LEN {
        return Err(Error::LinkKey);
    }
    SecretKey::from_slice(&bytes).map_err(|_| Error::LinkKey)
}

/// The whole link: `{base}/l/{id}#{key}`. `base` is the server's public
/// address, without a trailing slash. Holds the key, so it is wiped on drop.
pub fn url(base: &str, link_id: &str, key: &SecretKey) -> Zeroizing<String> {
    let fragment = key_to_fragment(key);
    Zeroizing::new(format!(
        "{}/l/{link_id}#{}",
        base.trim_end_matches('/'),
        fragment.as_str()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Card, CustomField, Login, Passkey, Website};

    const ID: [u8; 16] = [7; 16];

    fn login() -> ItemContent {
        let mut content = ItemContent::new(
            "GitHub",
            ItemData::Login(Login {
                username: "juan".into(),
                password: "correct horse".into(),
                websites: vec![Website {
                    url: "https://github.com".into(),
                    ..Website::default()
                }],
                totp: Some("otpauth://totp/GitHub?secret=JBSWY3DPEHPK3PXP".into()),
                ..Login::default()
            }),
        );
        content.notes = "Work account".into();
        content.custom_fields.push(CustomField {
            label: "Recovery code".into(),
            value: "1234-5678".into(),
            hidden: true,
            ..CustomField::default()
        });
        content.custom_fields.push(CustomField {
            label: "Empty".into(),
            ..CustomField::default()
        });
        content
    }

    fn labels(snapshot: &LinkSnapshot) -> Vec<(&str, FieldStyle)> {
        snapshot
            .fields
            .iter()
            .map(|field| (field.label.as_str(), field.style))
            .collect()
    }

    #[test]
    fn a_login_keeps_its_fields_and_leaves_out_empty_ones() {
        let snapshot = LinkSnapshot::of(&login(), false).unwrap();
        assert_eq!(snapshot.title, "GitHub");
        assert_eq!(snapshot.kind, ItemKind::Login);
        assert_eq!(snapshot.notes, "Work account");
        assert_eq!(
            labels(&snapshot),
            [
                ("Username", FieldStyle::Text),
                ("Password", FieldStyle::Secret),
                ("Website", FieldStyle::Url),
                ("Recovery code", FieldStyle::Secret),
            ]
        );
        assert_eq!(snapshot.fields[1].value.expose(), "correct horse");
    }

    #[test]
    fn the_two_factor_key_goes_in_only_when_asked() {
        let snapshot = LinkSnapshot::of(&login(), true).unwrap();
        let totp = snapshot
            .fields
            .iter()
            .find(|field| field.style == FieldStyle::Totp)
            .unwrap();
        assert_eq!(totp.label, "Two-factor code");
        assert!(totp.value.expose().starts_with("otpauth://"));
    }

    #[test]
    fn a_card_shows_its_expiry_as_month_and_year() {
        let content = ItemContent::new(
            "Visa",
            ItemData::Card(Card {
                cardholder: "Juan".into(),
                number: "4111111111111111".into(),
                expiry: Some(Expiry {
                    month: 3,
                    year: 2029,
                }),
                ..Card::default()
            }),
        );
        let snapshot = LinkSnapshot::of(&content, false).unwrap();
        assert_eq!(
            labels(&snapshot),
            [
                ("Cardholder", FieldStyle::Text),
                ("Number", FieldStyle::Secret),
                ("Expires", FieldStyle::Text),
            ]
        );
        assert_eq!(snapshot.fields[2].value.expose(), "03/2029");
    }

    #[test]
    fn a_passkey_cannot_be_shared() {
        let content = ItemContent::new("Site", ItemData::Passkey(Passkey::default()));
        assert_eq!(
            LinkSnapshot::of(&content, true).unwrap_err(),
            Error::NotShareable
        );
    }

    #[test]
    fn seals_and_opens_with_the_fragment_key() {
        let snapshot = LinkSnapshot::of(&login(), true).unwrap();
        let (payload, key) = seal(&snapshot, ID).unwrap();
        let fragment = key_to_fragment(&key);
        assert_eq!(fragment.len(), 43);
        let again = key_from_fragment(&format!("#{}", fragment.as_str())).unwrap();
        assert_eq!(open(&again, ID, &payload).unwrap(), snapshot);
    }

    #[test]
    fn refuses_another_link_a_wrong_key_and_any_change() {
        let snapshot = LinkSnapshot::of(&login(), false).unwrap();
        let (payload, key) = seal(&snapshot, ID).unwrap();
        assert_eq!(open(&key, [8; 16], &payload).unwrap_err(), Error::LinkKey);
        let other = SecretKey::generate().unwrap();
        assert_eq!(open(&other, ID, &payload).unwrap_err(), Error::LinkKey);
        let mut tampered = payload.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        assert_eq!(open(&key, ID, &tampered).unwrap_err(), Error::LinkKey);
    }

    #[test]
    fn refuses_malformed_fragments() {
        assert_eq!(key_from_fragment("").unwrap_err(), Error::LinkKey);
        assert_eq!(key_from_fragment("#abc").unwrap_err(), Error::LinkKey);
        assert_eq!(
            key_from_fragment("not base64!").unwrap_err(),
            Error::LinkKey
        );
    }

    #[test]
    fn builds_the_whole_url() {
        let key = SecretKey::from_bytes([0; 32]);
        assert_eq!(
            url("https://kandado.example/", "0190-ab", &key).as_str(),
            "https://kandado.example/l/0190-ab#AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        );
    }

    /// A payload made once with a fixed key, so a change to the format or the
    /// context that would break links already sent is caught here.
    #[test]
    fn opens_a_frozen_payload() {
        let key = SecretKey::from_bytes([0x42; 32]);
        let payload = data_encoding::HEXLOWER
            .decode(FROZEN_PAYLOAD.as_bytes())
            .unwrap();
        let snapshot = open(&key, ID, &payload).unwrap();
        assert_eq!(snapshot, frozen_snapshot());
    }

    fn frozen_snapshot() -> LinkSnapshot {
        LinkSnapshot {
            title: "Wi-Fi".into(),
            kind: ItemKind::Password,
            fields: vec![LinkField::new("Password", "hunter2", FieldStyle::Secret)],
            notes: "Guest network".into(),
        }
    }

    const FROZEN_PAYLOAD: &str = "01a2de993ad94a0aca6fb87212a13af9c4e86b7aa614d4fadc9e3da155cc08dfb07d007830f4b3b322fb92b7e623284d1949624bab82fa94a62f2b8a57a99ce2a8c419994a9037deef738ec2f30aaa2563a34f28434ba9d2119d2f8fa6a27d56f4464853c37cacf04122487929be337698232158232797077f649a48bafd0eb2e6aaa191382411635038d78843ddb83788c62c322db94cfc72d0c2df6c77d09bbe2a077c1ddf5311";

    #[test]
    #[ignore = "prints a new frozen payload"]
    fn print_frozen_payload() {
        let key = SecretKey::from_bytes([0x42; 32]);
        let payload = seal_with(&key, &frozen_snapshot(), ID).unwrap();
        println!("{}", data_encoding::HEXLOWER.encode(&payload));
    }
}
