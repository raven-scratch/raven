//! Names raven-asm can write.
//!
//! Scratch lets a variable, a list, a custom block or a parameter be called
//! anything at all — `"foo`, `< Perfect`, `a&b` are names a real project holds —
//! while a raven-asm name is an identifier. A name that cannot be written is
//! replaced by a deterministic encoding of itself, so reversing the same file
//! twice writes the same source, for the same reason a raven-asm build produces
//! the same identifiers twice. An encoding is reversible by reading the hex, and
//! an encoded name is itself a legal identifier, so decoding and re-encoding a
//! project is stable.

use std::collections::HashSet;

/// Words that mean something to the raven-asm parser wherever a name may
/// appear, so a name equal to one of them is encoded rather than written bare.
pub const KEYWORDS: &[&str] = &[
    "at",
    "broadcast",
    "center",
    "continuous",
    "costume",
    "default",
    "else",
    "false",
    "global",
    "large",
    "list",
    "proc",
    "slider",
    "sound",
    "sprite",
    "stage",
    "true",
    "use",
    "var",
    "visible",
    "warp",
];

/// Prefix of an encoded name, so a reader can tell one from a name the project
/// really had.
pub const ENCODED_PREFIX: &str = "re_";

/// The names already handed out while reversing one target.
#[derive(Clone, Debug, Default)]
pub struct Names {
    used: HashSet<String>,
}

impl Names {
    pub fn new() -> Self {
        Names::default()
    }

    /// A set with every raven-asm keyword taken, which is what a name may not be.
    pub fn with_keywords() -> Self {
        let mut names = Names::new();
        names.reserve_all(KEYWORDS.iter().copied());
        names
    }

    /// Reserve a name so [`Names::claim`] never returns it.
    pub fn reserve(&mut self, name: &str) {
        self.used.insert(name.to_string());
    }

    pub fn reserve_all<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
        for name in names {
            self.reserve(name);
        }
    }

    /// Take a name for `original`, encoding it when it cannot be written.
    ///
    /// `seed` disambiguates two names that encode to the same string; it is the
    /// id the project already gave the declaration, so the answer depends only
    /// on the file being reversed.
    pub fn claim(&mut self, original: &str, seed: &str) -> String {
        let base = if is_identifier(original) && !is_keyword(original) {
            original.to_string()
        } else {
            encode(original)
        };
        if self.used.insert(base.clone()) {
            return base;
        }
        let mut round = 0u32;
        loop {
            let candidate = format!("{base}_{}", suffix(seed, round));
            if self.used.insert(candidate.clone()) {
                return candidate;
            }
            round += 1;
        }
    }
}

/// Whether `name` is a raven-asm keyword rather than an ordinary name.
pub fn is_keyword(name: &str) -> bool {
    KEYWORDS.contains(&name)
}

/// Whether `name` can be written as a bare raven-asm identifier.
pub fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The encoding of a name an identifier cannot hold: its UTF-8 bytes in hex.
pub fn encode(name: &str) -> String {
    let mut out = String::with_capacity(ENCODED_PREFIX.len() + name.len() * 2);
    out.push_str(ENCODED_PREFIX);
    for byte in name.as_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Six hex digits of a hash of `seed` and the collision round.
fn suffix(seed: &str, round: u32) -> String {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for byte in seed.as_bytes().iter().chain(round.to_le_bytes().iter()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    format!("{:06x}", hash & 0xFF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_names_are_kept() {
        let mut names = Names::with_keywords();
        assert_eq!(names.claim("score", "id1"), "score");
        assert_eq!(names.claim("_hidden_1", "id2"), "_hidden_1");
    }

    #[test]
    fn names_an_identifier_cannot_hold_are_encoded() {
        assert!(!is_identifier("my score"));
        assert!(!is_identifier("1st"));
        assert!(!is_identifier("\"foo"));
        let mut names = Names::with_keywords();
        assert_eq!(names.claim("my score", "id"), "re_6d792073636f7265");
        // The encoding is an identifier, so claiming it again is stable.
        let mut again = Names::with_keywords();
        assert_eq!(
            again.claim("re_6d792073636f7265", "id"),
            "re_6d792073636f7265"
        );
    }

    #[test]
    fn keywords_and_taken_names_are_moved_aside() {
        let mut names = Names::with_keywords();
        assert_eq!(names.claim("var", "id"), "re_766172");
        let first = names.claim("score", "id1");
        let second = names.claim("score", "id2");
        assert_ne!(first, second);
        assert_eq!(first, "score");
        assert!(second.starts_with("score_"), "{second}");
    }

    #[test]
    fn the_same_seed_gives_the_same_suffix() {
        let mut a = Names::with_keywords();
        let mut b = Names::with_keywords();
        a.claim("x", "one");
        b.claim("x", "one");
        assert_eq!(a.claim("x", "two"), b.claim("x", "two"));
    }
}
