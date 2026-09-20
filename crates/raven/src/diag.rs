//! Diagnostics, re-exported from `raven-scratch`.
//!
//! Positions, messages, source lines and the error type all live in
//! `raven-scratch`, because `raven-asm` reports errors the same way and there is
//! no reason for two spellings of a caret. What raven adds is one thing: a
//! [`Span`], which is a position *and* a length, so a diagnostic can underline
//! the expression it is about rather than a single character.

pub use raven_scratch::diag::{render_diag, Diag, Error, Level, Pos, Result, Source};

/// A byte range inside one source file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub pos: Pos,
    /// Length in characters, used for the caret run.
    pub len: u32,
}

impl Span {
    #[must_use]
    pub const fn new(pos: Pos, len: u32) -> Self {
        Self { pos, len }
    }

    /// A one-character span at `pos`, for "something is missing here".
    #[must_use]
    pub const fn point(pos: Pos) -> Self {
        Self { pos, len: 1 }
    }

    /// The larger of two spans, when a node covers both.
    #[must_use]
    pub fn merge(self, other: Span) -> Span {
        if self.pos.line == 0 {
            return other;
        }
        if self.pos.line != other.pos.line {
            return self;
        }
        let start = self.pos.col.min(other.pos.col);
        let end = (self.pos.col + self.len).max(other.pos.col + other.len);
        Span::new(
            Pos::new(self.pos.line, start),
            end.saturating_sub(start).max(1),
        )
    }
}

/// Anchor a diagnostic at a span, with the source line it came from.
#[must_use]
pub fn anchored(source: &Source, span: Span, message: impl Into<String>) -> Diag {
    source.error(span.pos, message).span(span.len.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchored_spans_carry_the_source_line() {
        let source = Source::new("a.rav", "let x = 1;\n");
        let diag = anchored(&source, Span::new(Pos::new(1, 9), 1), "unexpected");
        assert_eq!(diag.snippet.as_deref(), Some("let x = 1;"));
        let rendered = render_diag(&diag);
        assert!(rendered.contains("a.rav:1:9"), "{rendered}");
        assert!(rendered.contains("^"), "{rendered}");
    }

    #[test]
    fn spans_merge_to_cover_both() {
        let a = Span::new(Pos::new(1, 5), 2);
        let b = Span::new(Pos::new(1, 10), 3);
        assert_eq!(a.merge(b), Span::new(Pos::new(1, 5), 8));
    }
}
