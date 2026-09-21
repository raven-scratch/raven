//! Writing raven-asm source text.
//!
//! The lexer defines what a raven-asm string literal is; this is the other
//! direction, for everything that prints raven-asm instead of parsing it:
//! `raven expand`, the staging file a `raven build` hands to raven-asm, and the
//! decompiler. One definition means a name that is legal in one of them is
//! legal in all of them.

/// A raven-asm string literal, escaped the way [`crate::lexer`] reads it back.
#[must_use]
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{{{:X}}}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A number as raven-asm writes it. An integral value loses its `.0`, because
/// that is what the source meant and what the editor shows.
#[must_use]
pub fn number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_are_escaped_the_way_the_lexer_reads_them() {
        assert_eq!(quote("plain"), "\"plain\"");
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote("a\\b"), "\"a\\\\b\"");
        assert_eq!(quote("a\nb\tc\rd"), "\"a\\nb\\tc\\rd\"");
        assert_eq!(quote("\u{1}"), "\"\\u{1}\"");
        assert_eq!(quote("é"), "\"é\"");
    }

    #[test]
    fn integral_numbers_lose_their_fraction() {
        assert_eq!(number(5.0), "5");
        assert_eq!(number(-0.0), "0");
        assert_eq!(number(1.5), "1.5");
    }
}
