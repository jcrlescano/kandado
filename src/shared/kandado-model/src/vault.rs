//! What a vault is called and how it looks: its name, colour and icon.
//!
//! Encoded form: `format version (1 byte) || JSON`, encrypted with the vault
//! key. The server never sees a vault's name.
//!
//! The same compatibility rules as items apply: every field has a default and
//! unknown fields are kept and written back.

use serde::{Deserialize, Serialize};

use crate::{Error, Result, item::Extra};

/// Version of the encoding described in this module.
pub const FORMAT_VERSION: u8 = 1;

/// The colours a vault can have, by their design token name.
pub const COLOURS: [&str; 6] = ["brass", "teal", "coral", "sky", "plum", "moss"];

/// The icons a vault can have, by their design system icon name.
pub const ICONS: [&str; 8] = [
    "vault", "user", "bolt", "card", "star", "shield", "globe", "key",
];

/// A vault's name, colour and icon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultMeta {
    #[serde(default)]
    pub name: String,
    /// One of [`COLOURS`]. Anything else is drawn as the first.
    #[serde(default = "default_colour")]
    pub colour: String,
    /// One of [`ICONS`]. Anything else is drawn as the first.
    #[serde(default = "default_icon")]
    pub icon: String,
    #[serde(flatten)]
    pub extra: Extra,
}

fn default_colour() -> String {
    COLOURS[0].to_owned()
}

fn default_icon() -> String {
    ICONS[0].to_owned()
}

impl Default for VaultMeta {
    fn default() -> Self {
        Self {
            name: String::new(),
            colour: default_colour(),
            icon: default_icon(),
            extra: Extra::new(),
        }
    }
}

impl VaultMeta {
    pub fn new(name: impl Into<String>, colour: &str, icon: &str) -> Self {
        Self {
            name: name.into(),
            colour: colour.to_owned(),
            icon: icon.to_owned(),
            extra: Extra::new(),
        }
    }

    /// The bytes to encrypt with the vault key.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut bytes = vec![FORMAT_VERSION];
        serde_json::to_writer(&mut bytes, self).map_err(|_| Error::Format)?;
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let (&version, json) = bytes.split_first().ok_or(Error::Format)?;
        if version != FORMAT_VERSION {
            return Err(Error::UnsupportedVersion(version));
        }
        serde_json::from_slice(json).map_err(|_| Error::Format)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_the_version_1_encoding_is_frozen() {
        let meta = VaultMeta::new("Family", "coral", "user");
        let bytes = meta.encode().unwrap();
        assert_eq!(
            bytes,
            b"\x01{\"name\":\"Family\",\"colour\":\"coral\",\"icon\":\"user\"}"
        );
        assert_eq!(VaultMeta::decode(&bytes).unwrap(), meta);
    }

    #[test]
    fn missing_fields_take_defaults_and_unknown_ones_survive() {
        let meta = VaultMeta::decode(b"\x01{\"name\":\"Work\",\"shared_hint\":true}").unwrap();
        assert_eq!(meta.colour, "brass");
        assert_eq!(meta.icon, "vault");
        let written: serde_json::Value =
            serde_json::from_slice(&meta.encode().unwrap()[1..]).unwrap();
        assert_eq!(written["shared_hint"], true);
    }

    #[test]
    fn refuses_unknown_versions_and_garbage() {
        assert_eq!(
            VaultMeta::decode(b"\x02{}").unwrap_err(),
            Error::UnsupportedVersion(2)
        );
        assert_eq!(VaultMeta::decode(b"").unwrap_err(), Error::Format);
        assert_eq!(VaultMeta::decode(b"\x01nope").unwrap_err(), Error::Format);
    }
}
