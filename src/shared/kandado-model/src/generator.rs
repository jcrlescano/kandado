//! The password generator: random characters or memorable words.
//!
//! Every choice is drawn from the system's secure random source with
//! rejection sampling, so each character and word is exactly equally likely.

use bip39::Language;
use kandado_crypto::random;

use crate::{Error, Result, Secret};

const LOWERCASE: &str = "abcdefghijkmnpqrstuvwxyz";
const UPPERCASE: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ";
const NUMBERS: &str = "23456789";
const SYMBOLS: &str = "!#$%&*+-=?@^_";

pub const MIN_LENGTH: u8 = 8;
pub const MAX_LENGTH: u8 = 64;
pub const MIN_WORDS: u8 = 3;
pub const MAX_WORDS: u8 = 8;

/// What to generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Options {
    /// Random characters. Lowercase letters are always used. Characters that
    /// are easy to misread (0 O o, 1 l I) are never used.
    Random {
        length: u8,
        uppercase: bool,
        numbers: bool,
        symbols: bool,
    },
    /// Random dictionary words.
    Memorable {
        words: u8,
        /// Start each word with a capital letter.
        capitalise: bool,
        /// Add one digit after the last word.
        numbers: bool,
        /// Join the words with hyphens instead of spaces.
        symbols: bool,
    },
}

impl Options {
    /// Random, 20 characters, uppercase, numbers and symbols on.
    pub const DEFAULT_RANDOM: Self = Self::Random {
        length: 20,
        uppercase: true,
        numbers: true,
        symbols: true,
    };

    /// Memorable, 4 words, capitalised, with a digit and hyphens.
    pub const DEFAULT_MEMORABLE: Self = Self::Memorable {
        words: 4,
        capitalise: true,
        numbers: true,
        symbols: true,
    };
}

impl Default for Options {
    fn default() -> Self {
        Self::DEFAULT_RANDOM
    }
}

/// A generated password and how hard it is to guess.
#[derive(Debug, Clone, PartialEq)]
pub struct Generated {
    pub password: Secret,
    /// Entropy in bits: the base-2 logarithm of how many passwords these
    /// options can produce. Exact, not an estimate.
    pub bits: f64,
}

/// Generates a password.
pub fn generate(options: &Options) -> Result<Generated> {
    match *options {
        Options::Random {
            length,
            uppercase,
            numbers,
            symbols,
        } => random_password(length, uppercase, numbers, symbols),
        Options::Memorable {
            words,
            capitalise,
            numbers,
            symbols,
        } => memorable_password(words, capitalise, numbers, symbols),
    }
}

fn pick(bound: usize) -> Result<usize> {
    let bound = u32::try_from(bound).map_err(|_| Error::GeneratorOptions)?;
    random::below(bound)
        .map(|index| index as usize)
        .map_err(|_| Error::Random)
}

fn random_password(length: u8, uppercase: bool, numbers: bool, symbols: bool) -> Result<Generated> {
    if !(MIN_LENGTH..=MAX_LENGTH).contains(&length) {
        return Err(Error::GeneratorOptions);
    }
    let classes: Vec<&str> = [
        (true, LOWERCASE),
        (uppercase, UPPERCASE),
        (numbers, NUMBERS),
        (symbols, SYMBOLS),
    ]
    .into_iter()
    .filter_map(|(enabled, class)| enabled.then_some(class))
    .collect();
    let pool: Vec<char> = classes.iter().flat_map(|class| class.chars()).collect();

    // Draw whole passwords until one has a character from every enabled
    // class, which sites commonly require. Redrawing the whole password keeps
    // all acceptable passwords equally likely. The first draw nearly always passes.
    loop {
        let mut password = String::with_capacity(length.into());
        for _ in 0..length {
            password.push(pool[pick(pool.len())?]);
        }
        let password = Secret::from(password);
        let complete = classes
            .iter()
            .all(|class| password.expose().chars().any(|c| class.contains(c)));
        if complete {
            return Ok(Generated {
                password,
                bits: random_bits(length, &classes),
            });
        }
    }
}

/// Counts the passwords of `length` over the pool that use every class
/// (inclusion-exclusion over the classes left out), and returns log2 of that.
fn random_bits(length: u8, classes: &[&str]) -> f64 {
    let sizes: Vec<f64> = classes.iter().map(|class| class.len() as f64).collect();
    let total: f64 = sizes.iter().sum();
    let mut count = 0.0;
    for mask in 0u32..(1 << sizes.len()) {
        let excluded: f64 = (0..sizes.len())
            .filter(|bit| mask & (1 << bit) != 0)
            .map(|bit| sizes[bit])
            .sum();
        let sign = if mask.count_ones() % 2 == 0 {
            1.0
        } else {
            -1.0
        };
        count += sign * (total - excluded).powi(length.into());
    }
    count.log2()
}

fn memorable_password(
    words: u8,
    capitalise: bool,
    numbers: bool,
    symbols: bool,
) -> Result<Generated> {
    if !(MIN_WORDS..=MAX_WORDS).contains(&words) {
        return Err(Error::GeneratorOptions);
    }
    let list = Language::English.word_list();
    let separator = if symbols { '-' } else { ' ' };

    let mut password = String::new();
    for index in 0..words {
        if index > 0 {
            password.push(separator);
        }
        let word = list[pick(list.len())?];
        let mut letters = word.chars();
        match letters.next() {
            Some(first) if capitalise => {
                password.push(first.to_ascii_uppercase());
                password.push_str(letters.as_str());
            }
            _ => password.push_str(word),
        }
    }
    let mut bits = f64::from(words) * (list.len() as f64).log2();
    if numbers {
        password.push(char::from(b'0' + pick(10)? as u8));
        bits += 10f64.log2();
    }
    Ok(Generated {
        password: password.into(),
        bits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const AMBIGUOUS: &str = "0Oo1lI";

    fn random(length: u8, uppercase: bool, numbers: bool, symbols: bool) -> Generated {
        generate(&Options::Random {
            length,
            uppercase,
            numbers,
            symbols,
        })
        .unwrap()
    }

    #[test]
    fn the_default_is_twenty_characters_from_every_class() {
        for _ in 0..200 {
            let generated = generate(&Options::default()).unwrap();
            let password = generated.password.expose();
            assert_eq!(password.chars().count(), 20);
            for class in [LOWERCASE, UPPERCASE, NUMBERS, SYMBOLS] {
                assert!(password.chars().any(|c| class.contains(c)));
            }
            assert!(!password.chars().any(|c| AMBIGUOUS.contains(c)));
        }
    }

    #[test]
    fn disabled_classes_never_appear() {
        for _ in 0..200 {
            let generated = random(32, false, false, false);
            assert!(
                generated
                    .password
                    .expose()
                    .chars()
                    .all(|c| LOWERCASE.contains(c))
            );
        }
        let numbers_only = random(32, false, true, false);
        assert!(
            !numbers_only
                .password
                .expose()
                .chars()
                .any(|c| UPPERCASE.contains(c) || SYMBOLS.contains(c))
        );
    }

    #[test]
    fn random_length_limits_are_enforced() {
        assert_eq!(
            random(MIN_LENGTH, true, true, true).password.expose().len(),
            8
        );
        assert_eq!(
            random(MAX_LENGTH, true, true, true).password.expose().len(),
            64
        );
        for length in [0, MIN_LENGTH - 1, MAX_LENGTH + 1] {
            let options = Options::Random {
                length,
                uppercase: true,
                numbers: true,
                symbols: true,
            };
            assert_eq!(generate(&options).unwrap_err(), Error::GeneratorOptions);
        }
    }

    #[test]
    fn random_entropy_counts_only_passwords_that_can_be_produced() {
        // One class: every string over the pool is acceptable.
        let lowercase_only = random(10, false, false, false).bits;
        assert!((lowercase_only - 10.0 * 24f64.log2()).abs() < 1e-9);
        // All classes: slightly below length * log2(pool), never above.
        let all = random(20, true, true, true).bits;
        let ceiling = 20.0 * 69f64.log2();
        assert!(all < ceiling && all > ceiling - 1.0, "{all} vs {ceiling}");
    }

    #[test]
    fn two_passwords_differ() {
        assert_ne!(
            generate(&Options::default()).unwrap().password,
            generate(&Options::default()).unwrap().password
        );
    }

    #[test]
    fn the_memorable_default_is_four_capitalised_words_and_a_digit() {
        let list = Language::English.word_list();
        for _ in 0..100 {
            let generated = generate(&Options::DEFAULT_MEMORABLE).unwrap();
            let password = generated.password.expose();
            let (body, digit) = password.split_at(password.len() - 1);
            assert!(digit.chars().all(|c| c.is_ascii_digit()));
            let words: Vec<_> = body.split('-').collect();
            assert_eq!(words.len(), 4);
            for word in words {
                assert!(word.starts_with(|c: char| c.is_ascii_uppercase()));
                assert!(list.contains(&word.to_ascii_lowercase().as_str()));
            }
            assert!((generated.bits - (44.0 + 10f64.log2())).abs() < 1e-9);
        }
    }

    #[test]
    fn memorable_options_change_the_shape() {
        let plain = generate(&Options::Memorable {
            words: 3,
            capitalise: false,
            numbers: false,
            symbols: false,
        })
        .unwrap();
        let password = plain.password.expose();
        assert_eq!(password.split(' ').count(), 3);
        assert!(password.chars().all(|c| c.is_ascii_lowercase() || c == ' '));
        assert!((plain.bits - 33.0).abs() < 1e-9);
    }

    #[test]
    fn memorable_word_limits_are_enforced() {
        for words in [0, MIN_WORDS - 1, MAX_WORDS + 1] {
            let options = Options::Memorable {
                words,
                capitalise: true,
                numbers: true,
                symbols: true,
            };
            assert_eq!(generate(&options).unwrap_err(), Error::GeneratorOptions);
        }
    }
}
