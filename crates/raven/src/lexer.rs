//! The raven lexer.
//!
//! One pass, no lookahead beyond a single character, and every token carries a
//! [`Span`] so a diagnostic can point at the exact characters that produced it.
//!
//! Two details are worth knowing before reading the code:
//!
//! * an interpolated string (`f"score: {n}"`) is lexed into *nested* tokens — the
//!   text between holes becomes [`Tok::InterpText`] and each hole becomes a
//!   `Vec<Token>` the parser hands to a sub-parser. That keeps the expression
//!   grammar in one place instead of inside a string scanner;
//! * `$name` is a single token, because it can only ever be a macro parameter and
//!   treating it as punctuation would let `$` leak into the grammar everywhere
//!   else.

use raven_scratch::diag::{Diag, Pos, Result, Source};

use crate::diag::Span;

/// A keyword. Every one of these is reserved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kw {
    Stage,
    Sprite,
    Use,
    Pub,
    Var,
    Let,
    Const,
    Proc,
    Fn,
    Macro,
    On,
    Warp,
    Broadcast,
    Costume,
    Sound,
    If,
    Else,
    Repeat,
    RepeatUntil,
    Forever,
    While,
    For,
    In,
    Match,
    Return,
    Num,
    Str,
    Bool,
    List,
    Map,
    Struct,
    Watch,
    True,
    False,
}

impl Kw {
    #[must_use]
    pub const fn text(self) -> &'static str {
        match self {
            Kw::Stage => "stage",
            Kw::Sprite => "sprite",
            Kw::Use => "use",
            Kw::Pub => "pub",
            Kw::Var => "var",
            Kw::Let => "let",
            Kw::Const => "const",
            Kw::Proc => "proc",
            Kw::Fn => "fn",
            Kw::Macro => "macro",
            Kw::On => "on",
            Kw::Warp => "warp",
            Kw::Broadcast => "broadcast",
            Kw::Costume => "costume",
            Kw::Sound => "sound",
            Kw::If => "if",
            Kw::Else => "else",
            Kw::Repeat => "repeat",
            Kw::RepeatUntil => "repeat_until",
            Kw::Forever => "forever",
            Kw::While => "while",
            Kw::For => "for",
            Kw::In => "in",
            Kw::Match => "match",
            Kw::Return => "return",
            Kw::Num => "num",
            Kw::Str => "str",
            Kw::Bool => "bool",
            Kw::List => "list",
            Kw::Map => "map",
            Kw::Struct => "struct",
            Kw::Watch => "watch",
            Kw::True => "true",
            Kw::False => "false",
        }
    }

    fn from(name: &str) -> Option<Kw> {
        Some(match name {
            "stage" => Kw::Stage,
            "sprite" => Kw::Sprite,
            "use" => Kw::Use,
            "pub" => Kw::Pub,
            "var" => Kw::Var,
            "let" => Kw::Let,
            "const" => Kw::Const,
            "proc" => Kw::Proc,
            "fn" => Kw::Fn,
            "macro" => Kw::Macro,
            "on" => Kw::On,
            "warp" => Kw::Warp,
            "broadcast" => Kw::Broadcast,
            "costume" => Kw::Costume,
            "sound" => Kw::Sound,
            "if" => Kw::If,
            "else" => Kw::Else,
            "repeat" => Kw::Repeat,
            "repeat_until" => Kw::RepeatUntil,
            "forever" => Kw::Forever,
            "while" => Kw::While,
            "for" => Kw::For,
            "in" => Kw::In,
            "match" => Kw::Match,
            "return" => Kw::Return,
            "num" => Kw::Num,
            "str" => Kw::Str,
            "bool" => Kw::Bool,
            "list" => Kw::List,
            "map" => Kw::Map,
            "struct" => Kw::Struct,
            "watch" => Kw::Watch,
            "true" => Kw::True,
            "false" => Kw::False,
            _ => return None,
        })
    }
}

/// Punctuation and operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P {
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semi,
    Colon,
    ColonColon,
    Eq,
    Arrow,
    FatArrow,
    Dot,
    DotDot,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    AndAnd,
    OrOr,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    Underscore,
}

impl P {
    #[must_use]
    pub const fn text(self) -> &'static str {
        match self {
            P::LParen => "(",
            P::RParen => ")",
            P::LBrace => "{",
            P::RBrace => "}",
            P::LBracket => "[",
            P::RBracket => "]",
            P::Comma => ",",
            P::Semi => ";",
            P::Colon => ":",
            P::ColonColon => "::",
            P::Eq => "=",
            P::Arrow => "->",
            P::FatArrow => "=>",
            P::DotDot => "..",
            P::Dot => ".",
            P::Plus => "+",
            P::Minus => "-",
            P::Star => "*",
            P::Slash => "/",
            P::Percent => "%",
            P::Bang => "!",
            P::AndAnd => "&&",
            P::OrOr => "||",
            P::EqEq => "==",
            P::NotEq => "!=",
            P::Lt => "<",
            P::Le => "<=",
            P::Gt => ">",
            P::Ge => ">=",
            P::Underscore => "_",
        }
    }
}

/// A token kind.
#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ident(String),
    /// The exact spelling from the source, so `1.50` stays `1.50`.
    Number(String),
    Str(String),
    /// `f"…"`: alternating text and holes.
    Interp(Vec<InterpPart>),
    /// `$name`, only meaningful inside a macro body.
    Param(String),
    Kw(Kw),
    Punct(P),
    Eof,
}

/// One piece of an interpolated string.
#[derive(Clone, Debug, PartialEq)]
pub enum InterpPart {
    Text(String),
    Hole(Vec<Token>),
}

impl Tok {
    /// How the token should be named in an error message.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Tok::Ident(name) => format!("`{name}`"),
            Tok::Number(text) => format!("`{text}`"),
            Tok::Str(_) => "a string".to_string(),
            Tok::Interp(_) => "an interpolated string".to_string(),
            Tok::Param(name) => format!("`${name}`"),
            Tok::Kw(kw) => format!("`{}`", kw.text()),
            Tok::Punct(p) => format!("`{}`", p.text()),
            Tok::Eof => "end of file".to_string(),
        }
    }
}

/// A token and where it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

impl Token {
    #[must_use]
    pub fn new(tok: Tok, span: Span) -> Self {
        Self { tok, span }
    }
}

/// A comment, kept only for the formatter and for `///` documentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    pub text: String,
    pub span: Span,
    /// `///`, as opposed to `//` or `/* */`.
    pub doc: bool,
}

/// Everything one file lexed to.
#[derive(Clone, Debug)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    /// `///` comments, in source order, each with the item it precedes.
    pub docs: Vec<Comment>,
}

/// Scan `source` into tokens.
pub fn lex(source: &Source) -> Result<Lexed> {
    let mut lexer = Lexer::new(source);
    lexer.run()
}

struct Lexer<'a> {
    src: &'a Source,
    bytes: &'a [u8],
    at: usize,
    line: u32,
    col: u32,
    out: Vec<Token>,
    docs: Vec<Comment>,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a Source) -> Self {
        Self {
            src,
            bytes: src.text.as_bytes(),
            at: 0,
            line: 1,
            col: 1,
            out: Vec::new(),
            docs: Vec::new(),
        }
    }

    fn run(&mut self) -> Result<Lexed> {
        // A UTF-8 BOM is not part of the language.
        if self.bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            self.at = 3;
        }
        loop {
            self.skip_trivia()?;
            let span = self.span_here(1);
            let Some(c) = self.peek() else {
                self.out.push(Token::new(Tok::Eof, span));
                return Ok(Lexed {
                    tokens: std::mem::take(&mut self.out),
                    docs: std::mem::take(&mut self.docs),
                });
            };
            let token = match c {
                'f' if self.peek_at(1) == Some('"') => {
                    self.bump();
                    self.string(true)?
                }
                'a'..='z' | 'A'..='Z' | '_' => self.ident()?,
                '0'..='9' => self.number()?,
                '"' => self.string(false)?,
                '$' => self.param()?,
                _ => self.punct()?,
            };
            self.out.push(token);
        }
    }

    // -- character helpers ------------------------------------------------

    fn peek(&self) -> Option<char> {
        self.src.text[self.at..].chars().next()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.src.text[self.at..].chars().nth(n)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn span_here(&self, len: u32) -> Span {
        Span::new(Pos::new(self.line, self.col), len)
    }

    fn error(&self, span: Span, message: impl Into<String>) -> Diag {
        self.src.error(span.pos, message).span(span.len.max(1))
    }

    // -- trivia -----------------------------------------------------------

    fn skip_trivia(&mut self) -> Result<()> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('/') if self.peek_at(1) == Some('/') => {
                    let start = self.span_here(1);
                    let doc = self.peek_at(2) == Some('/') && self.peek_at(3) != Some('/');
                    while self.peek_at(2) == Some('/') && self.peek_at(3) != Some('/') {
                        self.bump();
                    }
                    self.bump();
                    self.bump();
                    let mut text = String::new();
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        text.push(c);
                        self.bump();
                    }
                    self.docs.push(Comment {
                        text: text.trim_end().to_string(),
                        span: Span::new(start.pos, self.col.saturating_sub(start.pos.col).max(1)),
                        doc,
                    });
                }
                Some('/') if self.peek_at(1) == Some('*') => {
                    let start = self.span_here(2);
                    self.bump();
                    self.bump();
                    loop {
                        match self.peek() {
                            None => {
                                return Err(Diag::error("unterminated block comment")
                                    .at(
                                        self.src.path.clone(),
                                        start.pos,
                                        self.src.line_text(start.pos.line),
                                    )
                                    .span(2)
                                    .into())
                            }
                            Some('*') if self.peek_at(1) == Some('/') => {
                                self.bump();
                                self.bump();
                                break;
                            }
                            _ => {
                                self.bump();
                            }
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    // -- tokens -----------------------------------------------------------

    fn ident(&mut self) -> Result<Token> {
        let start = self.span_here(1);
        let mut name = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                name.push(c);
                self.bump();
            } else {
                break;
            }
        }
        let span = Span::new(start.pos, self.col.saturating_sub(start.pos.col).max(1));
        // `_` on its own is the wildcard in a `match` arm.
        if name == "_" {
            return Ok(Token::new(Tok::Punct(P::Underscore), span));
        }
        match Kw::from(&name) {
            Some(kw) => Ok(Token::new(Tok::Kw(kw), span)),
            None => Ok(Token::new(Tok::Ident(name), span)),
        }
    }

    fn number(&mut self) -> Result<Token> {
        let start = self.span_here(1);
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                text.push(c);
                self.bump();
            } else {
                break;
            }
        }
        if self.peek() == Some('.') && self.peek_at(1) != Some('.') {
            text.push('.');
            self.bump();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    text.push(c);
                    self.bump();
                } else {
                    break;
                }
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            let sign = usize::from(matches!(self.peek_at(1), Some('+' | '-')));
            if matches!(self.peek_at(1 + sign), Some(c) if c.is_ascii_digit()) {
                text.push(self.bump().expect("checked"));
                if sign == 1 {
                    text.push(self.bump().expect("checked"));
                }
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        text.push(c);
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
        }
        Ok(Token::new(
            Tok::Number(text),
            Span::new(start.pos, self.col.saturating_sub(start.pos.col).max(1)),
        ))
    }

    fn param(&mut self) -> Result<Token> {
        let start = self.span_here(2);
        self.bump();
        let mut name = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                name.push(c);
                self.bump();
            } else {
                break;
            }
        }
        if name.is_empty() {
            return Err(self
                .error(
                    Span::new(start.pos, 1),
                    "expected a macro parameter name after `$`",
                )
                .into());
        }
        Ok(Token::new(
            Tok::Param(name),
            Span::new(start.pos, self.col.saturating_sub(start.pos.col).max(1)),
        ))
    }

    fn string(&mut self, interpolated: bool) -> Result<Token> {
        let start = self.span_here(1);
        self.bump();
        if !interpolated {
            let mut text = String::new();
            loop {
                match self.bump() {
                    None => return Err(self.unterminated(start)),
                    Some('"') => break,
                    Some('\\') => text.push(self.escape()?),
                    Some(c) => text.push(c),
                }
            }
            return Ok(Token::new(
                Tok::Str(text),
                Span::new(start.pos, self.col.saturating_sub(start.pos.col).max(1)),
            ));
        }

        let mut parts: Vec<InterpPart> = Vec::new();
        let mut text = String::new();
        loop {
            match self.bump() {
                None => return Err(self.unterminated(start)),
                Some('"') => break,
                Some('\\') => text.push(self.escape()?),
                Some('{') => {
                    if self.peek() == Some('{') {
                        self.bump();
                        text.push('{');
                        continue;
                    }
                    if !text.is_empty() {
                        parts.push(InterpPart::Text(std::mem::take(&mut text)));
                    }
                    let hole = self.hole(start)?;
                    parts.push(InterpPart::Hole(hole));
                }
                Some('}') => {
                    if self.peek() == Some('}') {
                        self.bump();
                        text.push('}');
                        continue;
                    }
                    return Err(self
                        .error(
                            Span::new(start.pos, 1),
                            "a `}` in an interpolated string must be written `}}`",
                        )
                        .note("a lone `}` closes an interpolation hole that was never opened")
                        .into());
                }
                Some(c) => text.push(c),
            }
        }
        if !text.is_empty() {
            parts.push(InterpPart::Text(text));
        }
        Ok(Token::new(
            Tok::Interp(parts),
            Span::new(start.pos, self.col.saturating_sub(start.pos.col).max(1)),
        ))
    }

    fn unterminated(&self, start: Span) -> raven_scratch::diag::Error {
        self.error(start, "unterminated string literal").into()
    }

    fn escape(&mut self) -> Result<char> {
        let start = self.span_here(1);
        match self.bump() {
            None => Err(self.unterminated(start)),
            Some('\\') => Ok('\\'),
            Some('"') => Ok('"'),
            Some('n') => Ok('\n'),
            Some('r') => Ok('\r'),
            Some('t') => Ok('\t'),
            Some('0') => Ok('\0'),
            Some('{') => Ok('{'),
            Some('}') => Ok('}'),
            Some('u') => {
                if self.bump() != Some('{') {
                    return Err(self
                        .error(start, "`\\u` must be followed by `{`")
                        .note("the form is `\\u{1F600}`")
                        .into());
                }
                let mut hex = String::new();
                loop {
                    match self.bump() {
                        Some('}') => break,
                        Some(c) if c.is_ascii_hexdigit() => hex.push(c),
                        _ => return Err(self.error(start, "invalid `\\u{…}` escape").into()),
                    }
                }
                let code = u32::from_str_radix(&hex, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or_else(|| self.error(start, format!("`\\u{{{hex}}}` is not a character")))?;
                Ok(code)
            }
            Some(other) => Err(self
                .error(start, format!("unknown escape `\\{other}`"))
                .note("`\\\\`, `\\\"`, `\\n`, `\\r`, `\\t`, `\\0`, `\\{`, `\\}` and `\\u{…}` are the escapes raven has")
                .into()),
        }
    }

    /// Lex the inside of an interpolation hole, up to the matching `}`.
    fn hole(&mut self, start: Span) -> Result<Vec<Token>> {
        let saved = std::mem::take(&mut self.out);
        let mut depth = 0usize;
        let result: Result<()> = loop {
            self.skip_trivia()?;
            match self.peek() {
                None => {
                    break Err(self
                        .error(start, "unterminated `{` in an interpolated string")
                        .into())
                }
                Some('}') if depth == 0 => {
                    self.bump();
                    break Ok(());
                }
                Some('{') => {
                    depth += 1;
                    let token = self.punct()?;
                    self.out.push(token);
                }
                Some('}') => {
                    depth -= 1;
                    let token = self.punct()?;
                    self.out.push(token);
                }
                Some(c) => {
                    let token = match c {
                        'a'..='z' | 'A'..='Z' | '_' => self.ident()?,
                        '0'..='9' => self.number()?,
                        '"' => self.string(false)?,
                        '$' => self.param()?,
                        _ => self.punct()?,
                    };
                    self.out.push(token);
                }
            }
        };
        let mut hole = std::mem::replace(&mut self.out, saved);
        result?;
        // The sub-parser needs the same sentinel a file gets.
        hole.push(Token::new(Tok::Eof, self.span_here(1)));
        Ok(hole)
    }

    fn punct(&mut self) -> Result<Token> {
        let start = self.span_here(1);
        let c = self.bump().expect("caller checked");
        let (p, len) = match c {
            '(' => (P::LParen, 1),
            ')' => (P::RParen, 1),
            '{' => (P::LBrace, 1),
            '}' => (P::RBrace, 1),
            '[' => (P::LBracket, 1),
            ']' => (P::RBracket, 1),
            ',' => (P::Comma, 1),
            ';' => (P::Semi, 1),
            '+' => (P::Plus, 1),
            '*' => (P::Star, 1),
            '/' => (P::Slash, 1),
            '%' => (P::Percent, 1),
            ':' if self.peek() == Some(':') => {
                self.bump();
                (P::ColonColon, 2)
            }
            ':' => (P::Colon, 1),
            '=' if self.peek() == Some('=') => {
                self.bump();
                (P::EqEq, 2)
            }
            '=' if self.peek() == Some('>') => {
                self.bump();
                (P::FatArrow, 2)
            }
            '=' => (P::Eq, 1),
            '-' if self.peek() == Some('>') => {
                self.bump();
                (P::Arrow, 2)
            }
            '-' => (P::Minus, 1),
            '.' if self.peek() == Some('.') => {
                self.bump();
                (P::DotDot, 2)
            }
            '.' => (P::Dot, 1),
            '!' if self.peek() == Some('=') => {
                self.bump();
                (P::NotEq, 2)
            }
            '!' => (P::Bang, 1),
            '&' if self.peek() == Some('&') => {
                self.bump();
                (P::AndAnd, 2)
            }
            '|' if self.peek() == Some('|') => {
                self.bump();
                (P::OrOr, 2)
            }
            '<' if self.peek() == Some('=') => {
                self.bump();
                (P::Le, 2)
            }
            '<' => (P::Lt, 1),
            '>' if self.peek() == Some('=') => {
                self.bump();
                (P::Ge, 2)
            }
            '>' => (P::Gt, 1),
            '&' | '|' => {
                return Err(self
                    .error(start, format!("unexpected `{c}`"))
                    .note(format!("did you mean `{c}{c}`?"))
                    .into())
            }
            other => {
                return Err(self
                    .error(start, format!("unexpected character `{other}`"))
                    .into())
            }
        };
        Ok(Token::new(
            Tok::Punct(p),
            Span::new(start.pos, u32::from(len as u16)),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(text: &str) -> Vec<Tok> {
        let source = Source::new("t.rav", text);
        lex(&source)
            .expect("lexes")
            .tokens
            .into_iter()
            .map(|t| t.tok)
            .collect()
    }

    #[test]
    fn keywords_and_identifiers_are_told_apart() {
        assert_eq!(
            toks("sprite Player"),
            [Tok::Kw(Kw::Sprite), Tok::Ident("Player".into()), Tok::Eof]
        );
    }

    #[test]
    fn numbers_keep_their_spelling() {
        assert_eq!(
            toks("1.50 1e3 10"),
            [
                Tok::Number("1.50".into()),
                Tok::Number("1e3".into()),
                Tok::Number("10".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn operators_prefer_the_longest_match() {
        assert_eq!(
            toks("<= >= == != -> => :: .."),
            [
                Tok::Punct(P::Le),
                Tok::Punct(P::Ge),
                Tok::Punct(P::EqEq),
                Tok::Punct(P::NotEq),
                Tok::Punct(P::Arrow),
                Tok::Punct(P::FatArrow),
                Tok::Punct(P::ColonColon),
                Tok::Punct(P::DotDot),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn an_interpolated_string_splits_into_text_and_holes() {
        let Tok::Interp(parts) = &toks("f\"a{n}b\"")[0] else {
            panic!("expected an interpolated string");
        };
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], InterpPart::Text("a".into()));
        assert!(matches!(parts[1], InterpPart::Hole(_)));
        assert_eq!(parts[2], InterpPart::Text("b".into()));
    }

    #[test]
    fn a_doubled_brace_is_a_literal_brace() {
        let Tok::Interp(parts) = &toks("f\"{{x}}\"")[0] else {
            panic!("expected an interpolated string");
        };
        assert_eq!(parts, &[InterpPart::Text("{x}".into())]);
    }

    #[test]
    fn escapes_are_cooked_once() {
        assert_eq!(toks(r#""a\n\"b""#)[0], Tok::Str("a\n\"b".into()));
    }

    #[test]
    fn comments_are_trivia_and_doc_comments_are_kept() {
        let source = Source::new("t.rav", "/// doc\n// plain\n/* block */ x");
        let lexed = lex(&source).expect("lexes");
        assert_eq!(lexed.tokens[0].tok, Tok::Ident("x".into()));
        assert_eq!(lexed.docs.len(), 2);
        assert!(lexed.docs[0].doc);
        assert_eq!(lexed.docs[0].text.trim(), "doc");
        assert!(!lexed.docs[1].doc);
    }

    #[test]
    fn diagnostic_cases() {
        for (text, expected) in [
            ("\"abc", "unterminated string"),
            ("/* abc", "unterminated block comment"),
            ("a & b", "did you mean `&&`"),
            ("$", "expected a macro parameter name"),
            ("?", "unexpected character `?`"),
        ] {
            let source = Source::new("t.rav", text);
            let error = lex(&source).expect_err(text);
            assert!(
                error.render().contains(expected),
                "`{text}` produced:\n{}",
                error.render()
            );
        }
    }

    #[test]
    fn spans_point_at_the_token() {
        let source = Source::new("t.rav", "let x = 12;");
        let lexed = lex(&source).expect("lexes");
        assert_eq!(lexed.tokens[3].tok, Tok::Number("12".into()));
        assert_eq!(lexed.tokens[3].span.pos, Pos::new(1, 9));
        assert_eq!(lexed.tokens[3].span.len, 2);
    }
}
