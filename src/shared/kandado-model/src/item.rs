//! The seven item kinds and how an item is turned into bytes.
//!
//! Encoded form: `format version (1 byte) || JSON`. The whole thing is
//! encrypted with the item key; the server only ever sees the kind.
//!
//! ```json
//! {"title":"GitHub","notes":"","favourite":false,"custom_fields":[],
//!  "kind":"login","data":{"username":"juan","password":"…","websites":[…]}}
//! ```
//!
//! Compatibility rules, which the tests enforce:
//! - Every field has a default, so an item written by an older client still reads.
//! - Fields this client does not know are kept and written back, so editing an
//!   item with an older client does not strip what a newer one added.
//! - A kind this client does not know is refused, not guessed at.

use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser::SerializeMap};
use serde_json::{Map, Value};
use zeroize::Zeroizing;

use crate::{Error, Result, Secret, url_match::MatchMode};

/// Version of the encoding described in this module.
pub const FORMAT_VERSION: u8 = 1;

/// Fields from a newer client, carried through untouched.
pub type Extra = Map<String, Value>;

/// The kind of an item. This is the one thing about an item the server can see.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Login,
    Alias,
    Card,
    Note,
    Identity,
    Password,
    Passkey,
}

impl ItemKind {
    pub const ALL: [Self; 7] = [
        Self::Login,
        Self::Alias,
        Self::Card,
        Self::Note,
        Self::Identity,
        Self::Password,
        Self::Passkey,
    ];

    /// The name used in the API and the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Alias => "alias",
            Self::Card => "card",
            Self::Note => "note",
            Self::Identity => "identity",
            Self::Password => "password",
            Self::Passkey => "passkey",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == name)
    }
}

/// Everything inside an item's encrypted content.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemContent {
    pub title: String,
    /// Free text. For a note item this is the note itself.
    pub notes: String,
    pub favourite: bool,
    pub custom_fields: Vec<CustomField>,
    pub data: ItemData,
    pub extra: Extra,
}

// Written by hand so the kind-specific fields go straight from their struct
// to the output, without an intermediate copy that would not be wiped.
impl Serialize for ItemContent {
    fn serialize<S: Serializer>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("title", &self.title)?;
        map.serialize_entry("notes", &self.notes)?;
        map.serialize_entry("favourite", &self.favourite)?;
        map.serialize_entry("custom_fields", &self.custom_fields)?;
        map.serialize_entry("kind", &self.kind())?;
        match &self.data {
            ItemData::Login(data) => map.serialize_entry("data", data)?,
            ItemData::Alias(data) => map.serialize_entry("data", data)?,
            ItemData::Card(data) => map.serialize_entry("data", data)?,
            ItemData::Note(data) => map.serialize_entry("data", data)?,
            ItemData::Identity(data) => map.serialize_entry("data", data)?,
            ItemData::Password(data) => map.serialize_entry("data", data)?,
            ItemData::Passkey(data) => map.serialize_entry("data", data)?,
        }
        for (key, value) in &self.extra {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for ItemContent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> core::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            title: String,
            #[serde(default)]
            notes: String,
            #[serde(default)]
            favourite: bool,
            #[serde(default)]
            custom_fields: Vec<CustomField>,
            kind: ItemKind,
            #[serde(default)]
            data: Value,
            #[serde(flatten)]
            extra: Extra,
        }

        fn typed<T: de::DeserializeOwned, E: de::Error>(
            value: Value,
        ) -> core::result::Result<T, E> {
            let value = if value.is_null() {
                Value::Object(Map::new())
            } else {
                value
            };
            // `from_value` moves strings out of the value, so secrets are not copied here.
            serde_json::from_value(value).map_err(E::custom)
        }

        let raw = Raw::deserialize(deserializer)?;
        let data = match raw.kind {
            ItemKind::Login => ItemData::Login(typed(raw.data)?),
            ItemKind::Alias => ItemData::Alias(typed(raw.data)?),
            ItemKind::Card => ItemData::Card(typed(raw.data)?),
            ItemKind::Note => ItemData::Note(typed(raw.data)?),
            ItemKind::Identity => ItemData::Identity(Box::new(typed(raw.data)?)),
            ItemKind::Password => ItemData::Password(typed(raw.data)?),
            ItemKind::Passkey => ItemData::Passkey(typed(raw.data)?),
        };
        Ok(Self {
            title: raw.title,
            notes: raw.notes,
            favourite: raw.favourite,
            custom_fields: raw.custom_fields,
            data,
            extra: raw.extra,
        })
    }
}

impl ItemContent {
    pub fn new(title: impl Into<String>, data: ItemData) -> Self {
        Self {
            title: title.into(),
            data,
            ..Self::default()
        }
    }

    pub fn kind(&self) -> ItemKind {
        self.data.kind()
    }

    /// The bytes to encrypt with the item key. Wiped from memory when dropped.
    pub fn encode(&self) -> Result<Zeroizing<Vec<u8>>> {
        let mut bytes = Zeroizing::new(vec![FORMAT_VERSION]);
        serde_json::to_writer(&mut *bytes, self).map_err(|_| Error::Format)?;
        Ok(bytes)
    }

    /// Records when a login's password was set (Monitor compares it with
    /// breach dates). A new login with a password, or a password that differs
    /// from `previous`, is stamped `now`; otherwise the earlier stamp is kept.
    pub fn note_password_change(&mut self, previous: Option<&ItemContent>, now: i64) {
        let ItemData::Login(login) = &mut self.data else {
            return;
        };
        let before = previous.and_then(|previous| match &previous.data {
            ItemData::Login(before) => Some(before),
            _ => None,
        });
        match before {
            Some(before) if before.password == login.password => {
                login.password_changed_at =
                    login.password_changed_at.or(before.password_changed_at);
            }
            Some(_) => login.password_changed_at = Some(now),
            None if !login.password.is_empty() && login.password_changed_at.is_none() => {
                login.password_changed_at = Some(now);
            }
            None => {}
        }
    }

    /// Reads bytes made by [`ItemContent::encode`], by this or another version of Kandado.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let (&version, json) = bytes.split_first().ok_or(Error::Format)?;
        if version != FORMAT_VERSION {
            return Err(Error::UnsupportedVersion(version));
        }
        serde_json::from_slice(json).map_err(|_| Error::Format)
    }
}

/// The fields that depend on the kind.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemData {
    Login(Login),
    Alias(Alias),
    Card(Card),
    Note(Note),
    /// Boxed because it is several times larger than the other kinds.
    Identity(Box<Identity>),
    Password(Password),
    Passkey(Passkey),
}

impl ItemData {
    pub fn kind(&self) -> ItemKind {
        match self {
            Self::Login(_) => ItemKind::Login,
            Self::Alias(_) => ItemKind::Alias,
            Self::Card(_) => ItemKind::Card,
            Self::Note(_) => ItemKind::Note,
            Self::Identity(_) => ItemKind::Identity,
            Self::Password(_) => ItemKind::Password,
            Self::Passkey(_) => ItemKind::Passkey,
        }
    }
}

impl Default for ItemData {
    fn default() -> Self {
        Self::Note(Note::default())
    }
}

/// A field a person added to an item.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CustomField {
    #[serde(default)]
    pub label: String,
    /// Always handled as a secret; `hidden` only says whether it starts masked.
    #[serde(default)]
    pub value: Secret,
    #[serde(default)]
    pub hidden: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A username and password for one or more websites.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Login {
    /// Username or email, whichever the site asks for.
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: Secret,
    #[serde(default)]
    pub websites: Vec<Website>,
    /// The two-factor setup key as an `otpauth://` URI (see [`crate::totp`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totp: Option<Secret>,
    /// When the password was last set, in Unix seconds. Monitor compares it
    /// with breach dates (D-36). Absent on items saved before it existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_changed_at: Option<i64>,
    /// Breaches (Have I Been Pwned names) the person marked as resolved for
    /// this login (D-36 "Mark as resolved").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolved_breaches: Vec<String>,
    /// The person said this login will not get two-factor (D-39 "Not for this item").
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub no_two_factor: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A website a login belongs to and how strictly to match it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Website {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub match_mode: MatchMode,
    #[serde(flatten)]
    pub extra: Extra,
}

/// An email alias. Forwarding, the mailbox and the on/off switch live on the
/// server, which has to act on them; the item holds what the person keeps.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Alias {
    /// The alias address, such as `shop.k3x@kandado.example`.
    #[serde(default)]
    pub address: String,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A payment card.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Card {
    #[serde(default)]
    pub cardholder: String,
    #[serde(default)]
    pub number: Secret,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry: Option<Expiry>,
    #[serde(default)]
    pub cvv: Secret,
    #[serde(default)]
    pub pin: Secret,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A card's expiry month.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expiry {
    /// 1 to 12.
    pub month: u8,
    /// Four digits, such as 2028.
    pub year: u16,
}

/// A note. The text is [`ItemContent::notes`]; a note has no fields of its own.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Note {
    #[serde(flatten)]
    pub extra: Extra,
}

/// Personal details for filling address and sign-up forms.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Identity {
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub middle_name: String,
    #[serde(default)]
    pub last_name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub company: String,
    #[serde(default)]
    pub job_title: String,
    #[serde(default)]
    pub address: Address,
    /// Passport, licence, tax and similar numbers.
    #[serde(default)]
    pub documents: Vec<Document>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Address {
    #[serde(default)]
    pub line1: String,
    #[serde(default)]
    pub line2: String,
    #[serde(default)]
    pub city: String,
    /// State, province or region.
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub postal_code: String,
    #[serde(default)]
    pub country: String,
    #[serde(flatten)]
    pub extra: Extra,
}

/// An identity document: what it is and its number.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Document {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub number: Secret,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A generated password saved on its own, not yet attached to a login.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Password {
    #[serde(default)]
    pub password: Secret,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A WebAuthn credential. Binary values are base64url without padding, as in
/// the WebAuthn API.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Passkey {
    /// The relying party ID, such as `github.com`.
    #[serde(default)]
    pub rp_id: String,
    #[serde(default)]
    pub rp_name: String,
    #[serde(default)]
    pub credential_id: String,
    #[serde(default)]
    pub user_handle: String,
    #[serde(default)]
    pub user_name: String,
    #[serde(default)]
    pub user_display_name: String,
    /// COSE algorithm identifier; -7 is ES256.
    #[serde(default)]
    pub algorithm: i32,
    /// The private key, PKCS #8 DER, base64url.
    #[serde(default)]
    pub private_key: Secret,
    #[serde(default)]
    pub sign_count: u32,
    #[serde(flatten)]
    pub extra: Extra,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples() -> Vec<ItemContent> {
        vec![
            ItemContent {
                title: "GitHub".into(),
                notes: "Work account".into(),
                favourite: true,
                custom_fields: vec![CustomField {
                    label: "Recovery code".into(),
                    value: "abcd-efgh".into(),
                    hidden: true,
                    extra: Extra::new(),
                }],
                data: ItemData::Login(Login {
                    username: "juan@example.com".into(),
                    password: "correct horse".into(),
                    websites: vec![Website {
                        url: "https://github.com/login".into(),
                        match_mode: MatchMode::Host,
                        extra: Extra::new(),
                    }],
                    totp: Some("otpauth://totp/GitHub:juan?secret=JBSWY3DPEHPK3PXP".into()),
                    password_changed_at: None,
                    resolved_breaches: Vec::new(),
                    no_two_factor: false,
                    extra: Extra::new(),
                }),
                extra: Extra::new(),
            },
            ItemContent::new(
                "Shopping",
                ItemData::Alias(Alias {
                    address: "shop.k3x@kandado.example".into(),
                    extra: Extra::new(),
                }),
            ),
            ItemContent::new(
                "BPI Visa",
                ItemData::Card(Card {
                    cardholder: "Juan Dela Cruz".into(),
                    number: "4111111111111111".into(),
                    expiry: Some(Expiry {
                        month: 9,
                        year: 2028,
                    }),
                    cvv: "123".into(),
                    pin: "4821".into(),
                    extra: Extra::new(),
                }),
            ),
            ItemContent {
                title: "Wi-Fi at home".into(),
                notes: "Router is behind the TV.\nPassword: tubig-at-langis".into(),
                ..ItemContent::default()
            },
            ItemContent::new(
                "Me",
                ItemData::Identity(Box::new(Identity {
                    first_name: "Juan".into(),
                    last_name: "Dela Cruz".into(),
                    email: "juan@example.com".into(),
                    phone: "+63 917 000 0000".into(),
                    company: "Acme Studio".into(),
                    address: Address {
                        line1: "1 Ayala Ave".into(),
                        city: "Makati".into(),
                        postal_code: "1226".into(),
                        country: "PH".into(),
                        ..Address::default()
                    },
                    documents: vec![Document {
                        label: "Passport".into(),
                        number: "P1234567A".into(),
                        extra: Extra::new(),
                    }],
                    ..Identity::default()
                })),
            ),
            ItemContent::new(
                "Generated 12 Oct",
                ItemData::Password(Password {
                    password: "x7#Kq2mVp9".into(),
                    extra: Extra::new(),
                }),
            ),
            ItemContent::new(
                "GitHub passkey",
                ItemData::Passkey(Passkey {
                    rp_id: "github.com".into(),
                    rp_name: "GitHub".into(),
                    credential_id: "AQID".into(),
                    user_handle: "BAUG".into(),
                    user_name: "juan".into(),
                    user_display_name: "Juan Dela Cruz".into(),
                    algorithm: -7,
                    private_key: "MIGH".into(),
                    sign_count: 3,
                    extra: Extra::new(),
                }),
            ),
        ]
    }

    #[test]
    fn every_kind_round_trips() {
        let samples = samples();
        let kinds: Vec<_> = samples.iter().map(ItemContent::kind).collect();
        assert_eq!(kinds, ItemKind::ALL);
        for item in samples {
            let bytes = item.encode().unwrap();
            assert_eq!(bytes[0], FORMAT_VERSION);
            assert_eq!(ItemContent::decode(&bytes).unwrap(), item);
        }
    }

    #[test]
    fn kind_names_match_the_encoding_and_parse_back() {
        for item in samples() {
            let json: Value = serde_json::from_slice(&item.encode().unwrap()[1..]).unwrap();
            assert_eq!(json["kind"], item.kind().as_str());
            assert_eq!(ItemKind::parse(item.kind().as_str()), Some(item.kind()));
        }
        assert_eq!(ItemKind::parse("Login"), None);
    }

    /// The exact bytes of a version 1 login. If this fails, stored items no longer read.
    #[test]
    fn the_version_1_login_encoding_is_frozen() {
        let encoded = concat!(
            "\u{1}",
            r#"{"title":"GitHub","notes":"","favourite":false,"custom_fields":[],"#,
            r#""kind":"login","data":{"username":"juan","password":"pw","#,
            r#""websites":[{"url":"https://github.com","match_mode":"domain"}]}}"#,
        );
        let item = ItemContent::new(
            "GitHub",
            ItemData::Login(Login {
                username: "juan".into(),
                password: "pw".into(),
                websites: vec![Website {
                    url: "https://github.com".into(),
                    ..Website::default()
                }],
                ..Login::default()
            }),
        );
        assert_eq!(item.encode().unwrap().as_slice(), encoded.as_bytes());
        assert_eq!(ItemContent::decode(encoded.as_bytes()).unwrap(), item);
    }

    #[test]
    fn missing_fields_take_their_defaults() {
        let item = ItemContent::decode(b"\x01{\"kind\":\"card\"}").unwrap();
        assert_eq!(item, ItemContent::new("", ItemData::Card(Card::default())));
    }

    #[test]
    fn fields_from_a_newer_client_survive_an_edit() {
        let newer = concat!(
            "\u{1}",
            r#"{"title":"Bank","kind":"login","colour":"teal","data":{"username":"juan","#,
            r#""passkeys":[{"id":1}],"websites":[{"url":"https://bank.example","port_rule":"any"}]}}"#,
        );
        let mut item = ItemContent::decode(newer.as_bytes()).unwrap();
        item.title = "My bank".into();

        let written: Value = serde_json::from_slice(&item.encode().unwrap()[1..]).unwrap();
        assert_eq!(written["title"], "My bank");
        assert_eq!(written["colour"], "teal");
        assert_eq!(written["data"]["username"], "juan");
        assert_eq!(written["data"]["passkeys"][0]["id"], 1);
        assert_eq!(written["data"]["websites"][0]["port_rule"], "any");
        // Nothing is written twice or moved to another level.
        assert_eq!(written.as_object().unwrap().len(), 7);
        assert_eq!(written["data"].as_object().unwrap().len(), 4);
    }

    #[test]
    fn refuses_unknown_versions_unknown_kinds_and_garbage() {
        assert_eq!(
            ItemContent::decode(b"\x02{}").unwrap_err(),
            Error::UnsupportedVersion(2)
        );
        for bad in [
            &b""[..],
            b"\x01",
            b"\x01{}",
            b"\x01{\"kind\":\"hologram\"}",
            b"\x01not json",
        ] {
            assert_eq!(ItemContent::decode(bad).unwrap_err(), Error::Format);
        }
    }

    #[test]
    fn debug_output_shows_no_secret() {
        let debug = format!("{:?}", samples());
        for secret in [
            "correct horse",
            "JBSWY3DPEHPK3PXP",
            "abcd-efgh",
            "4111111111111111",
            "4821",
            "\"123\"",
            "P1234567A",
            "x7#Kq2mVp9",
            "MIGH",
        ] {
            assert!(!debug.contains(secret), "{secret} leaked into Debug output");
        }
    }
}
