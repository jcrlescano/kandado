//! The system's secure random source. On the web this is `crypto.getRandomValues`.

use crate::{Error, Result};

/// Fills `dest` with random bytes.
pub fn fill(dest: &mut [u8]) -> Result<()> {
    getrandom::fill(dest).map_err(|_| Error::Random)
}

/// A uniformly random number in `0..bound`. `bound` must not be zero.
///
/// Uses rejection sampling, so every value is exactly equally likely
/// (a plain `% bound` would favour small values).
pub fn below(bound: u32) -> Result<u32> {
    assert!(bound > 0, "bound must be positive");
    // The largest multiple of `bound` that fits; values at or above it are redrawn.
    let limit = u32::MAX - u32::MAX % bound;
    loop {
        let mut bytes = [0; 4];
        fill(&mut bytes)?;
        let value = u32::from_le_bytes(bytes);
        if value < limit {
            return Ok(value % bound);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_stays_in_range_and_reaches_every_value() {
        let mut seen = [false; 7];
        for _ in 0..2_000 {
            seen[below(7).unwrap() as usize] = true;
        }
        assert!(seen.iter().all(|&hit| hit));
        assert_eq!(below(1).unwrap(), 0);
    }
}
