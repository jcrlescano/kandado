use core::fmt;

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// Bytes the server stores and returns without reading: ciphertext, wrapped
/// keys, sealed keys. Base64 (standard alphabet, padded) on the wire.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Opaque(Vec<u8>);

impl Opaque {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<u8>> for Opaque {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

/// Shows the length only. The content is ciphertext, but logs have no use for it.
impl fmt::Debug for Opaque {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Opaque({} bytes)", self.0.len())
    }
}

impl Serialize for Opaque {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Opaque {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        STANDARD
            .decode(text.as_bytes())
            .map(Self)
            .map_err(|_| de::Error::custom("expected base64"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn travels_as_padded_standard_base64() {
        let opaque = Opaque::new(vec![0xFB, 0xFF, 0x00, 0x01]);
        assert_eq!(serde_json::to_string(&opaque).unwrap(), r#""+/8AAQ==""#);
        assert_eq!(
            serde_json::from_str::<Opaque>(r#""+/8AAQ==""#).unwrap(),
            opaque
        );
        assert_eq!(
            serde_json::from_str::<Opaque>(r#""""#).unwrap(),
            Opaque::default()
        );
    }

    #[test]
    fn rejects_anything_that_is_not_base64() {
        for bad in [
            r#""not base64!""#,
            r#""-_8AAQ==""#,
            r#""+/8AAQ""#,
            "42",
            "null",
        ] {
            assert!(serde_json::from_str::<Opaque>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn debug_output_shows_only_the_length() {
        assert_eq!(
            format!("{:?}", Opaque::new(vec![1, 2, 3])),
            "Opaque(3 bytes)"
        );
    }
}
