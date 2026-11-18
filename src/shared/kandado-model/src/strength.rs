//! Password strength as the 0 to 4 score the strength meter shows.
//!
//! The estimate comes from zxcvbn, which looks for dictionary words, names,
//! dates, keyboard walks, repeats and sequences, so a long but predictable
//! password still scores low. Length alone never decides the score.

use zxcvbn::matching::patterns::MatchPattern;

/// How strong a password is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strength {
    /// 0 to 4. The meter shows 0 and 1 as Weak, 2 Fair, 3 Good, 4 Strong.
    pub score: u8,
    /// Estimated guessing entropy in bits.
    pub bits: f64,
}

impl Strength {
    /// The word shown beside the meter.
    pub fn label(&self) -> &'static str {
        match self.score {
            0 | 1 => "Weak",
            2 => "Fair",
            3 => "Good",
            _ => "Strong",
        }
    }
}

/// The score for a number of bits of entropy. Used directly for generated
/// passwords, whose entropy is known exactly.
pub fn score_for_bits(bits: f64) -> u8 {
    match bits {
        b if b <= 0.0 => 0,
        b if b < 36.0 => 1,
        b if b < 60.0 => 2,
        b if b < 80.0 => 3,
        _ => 4,
    }
}

/// Estimates the strength of a password a person typed or imported.
///
/// `user_inputs` are things an attacker would try first: the account email,
/// the item's username, the site name. A password built from them scores lower.
pub fn estimate(password: &str, user_inputs: &[&str]) -> Strength {
    if password.is_empty() {
        return Strength {
            score: 0,
            bits: 0.0,
        };
    }
    let analysis = zxcvbn::zxcvbn(password, user_inputs);
    // Add up the parts zxcvbn split the password into. A part with a pattern
    // (word, date, repeat, keyboard walk) costs what zxcvbn says. A part with
    // no pattern behaves like random characters; zxcvbn counts those at a flat
    // 10 guesses each, which would rate a 20-character generated password
    // below "Strong", so those are counted by the character classes in use.
    let per_character = pool_bits(password);
    let bits: f64 = analysis
        .sequence()
        .iter()
        .map(|part| match (&part.pattern, part.guesses) {
            (MatchPattern::BruteForce, _) | (_, None) => {
                part.token.chars().count() as f64 * per_character
            }
            (_, Some(guesses)) => (guesses.max(1) as f64).log2(),
        })
        .sum();
    Strength {
        score: score_for_bits(bits),
        bits,
    }
}

/// Bits per character if each were drawn at random from the character
/// classes the password uses.
fn pool_bits(password: &str) -> f64 {
    let mut pool = 0u32;
    for (present, size) in [
        (password.chars().any(|c| c.is_ascii_lowercase()), 26),
        (password.chars().any(|c| c.is_ascii_uppercase()), 26),
        (password.chars().any(|c| c.is_ascii_digit()), 10),
        (
            password
                .chars()
                .any(|c| c.is_ascii_punctuation() || c == ' '),
            33,
        ),
        (!password.is_ascii(), 100),
    ] {
        if present {
            pool += size;
        }
    }
    f64::from(pool.max(2)).log2()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_thresholds_match_the_strength_meter() {
        let cases = [
            (0.0, 0),
            (1.0, 1),
            (35.9, 1),
            (36.0, 2),
            (59.9, 2),
            (60.0, 3),
            (79.9, 3),
            (80.0, 4),
            (300.0, 4),
        ];
        for (bits, score) in cases {
            assert_eq!(score_for_bits(bits), score, "{bits} bits");
        }
    }

    #[test]
    fn labels_follow_the_score() {
        let labels: Vec<_> = (0..=4)
            .map(|score| Strength { score, bits: 0.0 }.label())
            .collect();
        assert_eq!(labels, ["Weak", "Weak", "Fair", "Good", "Strong"]);
    }

    #[test]
    fn common_and_patterned_passwords_are_weak() {
        for password in ["password", "123456", "qwerty", "letmein", "iloveyou"] {
            assert!(estimate(password, &[]).score <= 1, "{password}");
        }
    }

    #[test]
    fn length_alone_does_not_make_a_password_strong() {
        let repeated = "a".repeat(40);
        assert!(estimate(&repeated, &[]).score <= 1);
        assert!(estimate("abcdefghijklmnopqrstuvwxyz", &[]).score <= 1);
        assert!(estimate("passwordpasswordpassword", &[]).score <= 2);
    }

    #[test]
    fn a_long_random_password_is_strong() {
        let strength = estimate("x7#Kq2mVp9!tRz4&Wn8$", &[]);
        assert_eq!(strength.score, 4);
        assert_eq!(strength.label(), "Strong");
    }

    #[test]
    fn a_short_random_password_is_not_strong() {
        assert!(estimate("x7#Kq2mV", &[]).score <= 2);
    }

    #[test]
    fn a_generated_password_scores_the_same_typed_as_generated() {
        use crate::generator::{Options, generate};
        for _ in 0..50 {
            let generated = generate(&Options::default()).unwrap();
            let typed = estimate(generated.password.expose(), &[]);
            assert_eq!(typed.score, score_for_bits(generated.bits));
        }
    }

    #[test]
    fn passwords_built_from_the_users_own_details_score_lower() {
        let alone = estimate("delacruzjuan", &[]);
        let known = estimate("delacruzjuan", &["juan", "delacruz"]);
        assert!(known.bits < alone.bits);
    }

    #[test]
    fn the_empty_password_scores_zero() {
        assert_eq!(
            estimate("", &[]),
            Strength {
                score: 0,
                bits: 0.0
            }
        );
    }
}
