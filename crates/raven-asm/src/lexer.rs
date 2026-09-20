//! Hand-written lexer for raven-asm source files.
//!
//! raven-asm's token set is deliberately tiny: identifiers (which include every
//! Scratch opcode and every contextual keyword), numbers, strings, and a
//! handful of punctuation characters. `//` and `/* */` comments are skipped.

use raven_scratch::diag::{Diag, Pos, Result, Source};

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ident(String),
    /// Raw source text of a numeric literal, exactly as it will be written into
    /// the Scratch project file.
    Number(String),
    Str(String),
    Punct(char),
    Eof,
}

impl Tok {
    pub fn describe(&self) -> String {
        match self {
            Tok::Ident(s) => format!("`{s}`"),
            Tok::Number(s) => format!("number `{s}`"),
            Tok::Str(s) => format!("string \"{s}\""),
            Tok::Punct(c) => format!("`{c}`"),
            Tok::Eof => "end of file".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub pos: Pos,
    /// Number of characters the token spans, for error underlining.
    pub len: u32,
}

pub struct Lexer<'a> {
    src: &'a Source,
    /// Byte offset of the current position.
    off: usize,
    line: u32,
    col: u32,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a Source) -> Self {
        let mut lexer = Lexer {
            src,
            off: 0,
            line: 1,
            col: 1,
        };
        // Skip a UTF-8 BOM if present so column numbers stay sane.
        if src.text.as_bytes().starts_with(&[0xEF, 0xBB, 0xBF]) {
            lexer.off = 3;
        }
        lexer
    }

    /// Tokenize the whole file, including a trailing `Eof` token.
    pub fn tokenize(mut self) -> Result<Vec<Token>> {
        let mut out = Vec::new();
        loop {
            self.skip_trivia()?;
            let pos = self.pos();
            let Some(c) = self.peek() else {
                out.push(Token {
                    tok: Tok::Eof,
                    pos,
                    len: 1,
                });
                return Ok(out);
            };
            let tok = if c == '"' {
                self.lex_string()?
            } else if c.is_ascii_digit()
                || (c == '.' && self.peek_at(1).is_some_and(|n| n.is_ascii_digit()))
            {
                self.lex_number()
            } else if is_ident_start(c) {
                self.lex_ident()
            } else if "(){}[],;=:".contains(c) {
                self.bump();
                Tok::Punct(c)
            } else if c == '-' {
                // `-` only ever starts a negative numeric literal.
                if self
                    .peek_at(1)
                    .is_some_and(|n| n.is_ascii_digit() || n == '.')
                {
                    self.lex_number()
                } else {
                    let d = self.diag_here("`-` may only introduce a negative number literal")
                        .note("Scratch has no negation operator; use `operator_subtract(0, x)` instead");
                    return Err(d.into());
                }
            } else {
                let d = self
                    .diag_here(format!("unexpected character `{c}`"))
                    .note("identifiers may contain letters, digits and `_`, and must not start with a digit");
                return Err(d.into());
            };
            let len = self.col.saturating_sub(pos.col).max(1);
            out.push(Token { tok, pos, len });
        }
    }

    fn pos(&self) -> Pos {
        Pos::new(self.line, self.col)
    }

    fn diag_here(&self, msg: impl Into<String>) -> Diag {
        self.src.error(self.pos(), msg)
    }

    fn peek(&self) -> Option<char> {
        self.src.text[self.off..].chars().next()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.src.text[self.off..].chars().nth(n)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.off += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn skip_trivia(&mut self) -> Result<()> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('/') if self.peek_at(1) == Some('/') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                Some('/') if self.peek_at(1) == Some('*') => {
                    let start = self.pos();
                    self.bump();
                    self.bump();
                    loop {
                        match self.peek() {
                            None => {
                                let d = self
                                    .src
                                    .error(start, "unterminated block comment")
                                    .note("close it with `*/`");
                                return Err(d.into());
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

    fn lex_ident(&mut self) -> Tok {
        let start = self.off;
        while self.peek().is_some_and(is_ident_continue) {
            self.bump();
        }
        Tok::Ident(self.src.text[start..self.off].to_string())
    }

    fn lex_number(&mut self) -> Tok {
        let start = self.off;
        if self.peek() == Some('-') {
            self.bump();
        }
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
        if self.peek() == Some('.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            let save = (self.off, self.line, self.col);
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.bump();
                }
            } else {
                // Not an exponent after all; rewind.
                self.off = save.0;
                self.line = save.1;
                self.col = save.2;
            }
        }
        Tok::Number(self.src.text[start..self.off].to_string())
    }

    fn lex_string(&mut self) -> Result<Tok> {
        let start_pos = self.pos();
        self.bump(); // opening quote
        let mut value = String::new();
        loop {
            match self.peek() {
                None | Some('\n') => {
                    let d = self
                        .src
                        .error(start_pos, "unterminated string literal")
                        .note("string literals may not span lines; close it with `\"`");
                    return Err(d.into());
                }
                Some('"') => {
                    self.bump();
                    return Ok(Tok::Str(value));
                }
                Some('\\') => {
                    self.bump();
                    let escape_pos = self.pos();
                    match self.bump() {
                        Some('\\') => value.push('\\'),
                        Some('"') => value.push('"'),
                        Some('n') => value.push('\n'),
                        Some('r') => value.push('\r'),
                        Some('t') => value.push('\t'),
                        Some('0') => value.push('\0'),
                        Some('u') => {
                            if self.peek() != Some('{') {
                                return Err(self
                                    .src
                                    .error(escape_pos, "expected `{` after `\\u`")
                                    .into());
                            }
                            self.bump();
                            let mut hex = String::new();
                            while let Some(c) = self.peek() {
                                if c == '}' {
                                    break;
                                }
                                hex.push(c);
                                self.bump();
                            }
                            if self.peek() != Some('}') {
                                return Err(self
                                    .src
                                    .error(escape_pos, "unterminated `\\u{...}` escape")
                                    .into());
                            }
                            self.bump();
                            let code = u32::from_str_radix(&hex, 16).ok();
                            match code.and_then(char::from_u32) {
                                Some(ch) => value.push(ch),
                                None => {
                                    return Err(self
                                        .src
                                        .error(
                                            escape_pos,
                                            format!("invalid unicode escape `\\u{{{hex}}}`"),
                                        )
                                        .into())
                                }
                            }
                        }
                        other => {
                            let shown = other.map(|c| c.to_string()).unwrap_or_default();
                            return Err(self
                                .src
                                .error(escape_pos, format!("unknown escape `\\{shown}`"))
                                .note("supported escapes: \\\\ \\\" \\n \\r \\t \\0 \\u{XXXX}")
                                .into());
                        }
                    }
                }
                Some(_) => {
                    value.push(self.bump().expect("peeked"));
                }
            }
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
