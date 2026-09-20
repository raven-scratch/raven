//! Deterministic Scratch-style identifier generation.
//!
//! Scratch uses 20-character "soup" identifiers for blocks, variables,
//! broadcasts and costumes. raven-asm derives them from a seed with a small PRNG
//! instead of a random source, so building the same project twice produces the
//! same bytes and `git diff` stays quiet.

/// Scratch's identifier alphabet, restricted to characters that are safe in
/// JSON keys, DOM ids and URLs.
const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

pub const ID_LEN: usize = 20;

#[derive(Debug, Default)]
pub struct IdGen {
    counter: u64,
}

impl IdGen {
    pub fn new() -> Self {
        IdGen { counter: 0 }
    }

    /// Produce a fresh identifier derived from `seed`. Two calls always return
    /// different identifiers, even for the same seed.
    pub fn fresh(&mut self, seed: &str) -> String {
        let n = self.counter;
        self.counter += 1;
        let mut state = fnv1a(seed.as_bytes()).wrapping_add(n.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut out = String::with_capacity(ID_LEN);
        while out.len() < ID_LEN {
            state = splitmix64(state);
            let mut bits = state;
            for _ in 0..10 {
                out.push(ALPHABET[(bits % ALPHABET.len() as u64) as usize] as char);
                bits /= ALPHABET.len() as u64;
                if out.len() == ID_LEN {
                    break;
                }
            }
        }
        out
    }
}

fn splitmix64(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_stable_and_unique() {
        let mut a = IdGen::new();
        let mut b = IdGen::new();
        let first: Vec<String> = (0..64).map(|_| a.fresh("seed")).collect();
        let second: Vec<String> = (0..64).map(|_| b.fresh("seed")).collect();
        assert_eq!(first, second, "generation must be reproducible");
        let unique: HashSet<&String> = first.iter().collect();
        assert_eq!(unique.len(), first.len(), "ids must be unique");
        for id in &first {
            assert_eq!(id.len(), ID_LEN);
            assert!(id.bytes().all(|c| ALPHABET.contains(&c)));
        }
    }
}
