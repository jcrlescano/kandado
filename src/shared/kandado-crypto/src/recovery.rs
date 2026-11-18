//! The recovery phrase: 12 English words (BIP39) that can unwrap the user key
//! when the master password is forgotten.
//!
//! The phrase encodes 128 bits of random entropy. Because that entropy is
//! uniformly random, a fast KDF is enough: the recovery key is
//! HKDF-SHA256(salt = "kandado/recovery/v1", ikm = entropy, info = "recovery-key").

use core::fmt;

use bip39::{Language, Mnemonic};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::{Error, KEY_LEN, Result, SecretKey, random};

pub const WORD_COUNT: usize = 12;
const ENTROPY_LEN: usize = 16;
const SALT: &[u8] = b"kandado/recovery/v1";
const INFO: &[u8] = b"recovery-key";

/// A recovery phrase. Wiped from memory when dropped; cannot be printed.
pub struct RecoveryPhrase(Mnemonic);

impl RecoveryPhrase {
    /// Creates a new random phrase.
    pub fn generate() -> Result<Self> {
        let mut entropy = Zeroizing::new([0u8; ENTROPY_LEN]);
        random::fill(entropy.as_mut_slice())?;
        Mnemonic::from_entropy_in(Language::English, entropy.as_slice())
            .map(Self)
            .map_err(|_| Error::InvalidPhrase)
    }

    /// Reads a phrase a person typed or pasted. Case and extra whitespace are
    /// ignored; the words and the checksum must be right.
    pub fn parse(text: &str) -> Result<Self> {
        let normalised = Zeroizing::new(
            text.split_whitespace()
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
                .join(" "),
        );
        let mnemonic = Mnemonic::parse_in_normalized(Language::English, &normalised)
            .map_err(|_| Error::InvalidPhrase)?;
        if mnemonic.word_count() != WORD_COUNT {
            return Err(Error::InvalidPhrase);
        }
        Ok(Self(mnemonic))
    }

    /// The words in order, for showing once at D-03 and for the saved file.
    pub fn words(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.0.words()
    }

    /// The key that wraps the user key for recovery.
    pub fn recovery_key(&self) -> Result<SecretKey> {
        let (entropy, len) = self.0.to_entropy_array();
        let entropy = Zeroizing::new(entropy);
        let mut key = Zeroizing::new([0u8; KEY_LEN]);
        Hkdf::<Sha256>::new(Some(SALT), &entropy[..len])
            .expand(INFO, key.as_mut_slice())
            .map_err(|_| Error::InvalidPhrase)?;
        Ok(SecretKey::from_bytes(*key))
    }
}

impl fmt::Debug for RecoveryPhrase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryPhrase(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BIP39 reference vector: all-zero entropy.
    const ZERO_PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn a_generated_phrase_has_twelve_words_and_parses_back() {
        let phrase = RecoveryPhrase::generate().unwrap();
        let words: Vec<_> = phrase.words().collect();
        assert_eq!(words.len(), WORD_COUNT);
        let parsed = RecoveryPhrase::parse(&words.join(" ")).unwrap();
        assert_eq!(
            parsed.recovery_key().unwrap().expose(),
            phrase.recovery_key().unwrap().expose()
        );
    }

    #[test]
    fn two_generated_phrases_differ() {
        let a: Vec<_> = RecoveryPhrase::generate().unwrap().words().collect();
        let b: Vec<_> = RecoveryPhrase::generate().unwrap().words().collect();
        assert_ne!(a, b);
    }

    #[test]
    fn parses_the_bip39_reference_phrase() {
        let phrase = RecoveryPhrase::parse(ZERO_PHRASE).unwrap();
        assert_eq!(phrase.words().collect::<Vec<_>>().join(" "), ZERO_PHRASE);
    }

    #[test]
    fn parsing_ignores_case_and_spacing() {
        let tidy = RecoveryPhrase::parse(ZERO_PHRASE).unwrap();
        let messy = RecoveryPhrase::parse(&format!(
            "  {}\n",
            ZERO_PHRASE.to_uppercase().replace(' ', "   ")
        ))
        .unwrap();
        assert_eq!(
            messy.recovery_key().unwrap().expose(),
            tidy.recovery_key().unwrap().expose()
        );
    }

    #[test]
    fn rejects_a_bad_checksum_an_unknown_word_and_the_wrong_length() {
        let bad_checksum = ZERO_PHRASE.replace("about", "abandon");
        let unknown_word = ZERO_PHRASE.replace("about", "kandado");
        // A valid 24-word BIP39 phrase, which Kandado never issues.
        let too_long = format!("{} art", ["abandon"; 23].join(" "));
        for text in ["", "abandon", &bad_checksum, &unknown_word, &too_long] {
            assert_eq!(
                RecoveryPhrase::parse(text).unwrap_err(),
                Error::InvalidPhrase,
                "{text:?}"
            );
        }
    }

    #[test]
    fn debug_output_never_shows_the_words() {
        let phrase = RecoveryPhrase::parse(ZERO_PHRASE).unwrap();
        assert_eq!(format!("{phrase:?}"), "RecoveryPhrase(..)");
    }
}
