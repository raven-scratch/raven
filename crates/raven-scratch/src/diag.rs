//! Source positions, diagnostics and the crate-wide error type.
//!
//! raven-asm reports errors the way a modern compiler does: a primary message with
//! a source location, the offending source line and a caret, plus optional
//! `= note:` lines that explain how to fix the problem.

use std::fmt;
use std::path::PathBuf;

/// A 1-based line/column position inside a single source file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
}

impl Pos {
    pub const fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }

    /// Prefer the more specific of two positions.
    ///
    /// An unset position is `0:0`, so it yields to a set one. This is how a
    /// parser records where a declaration really was when it has both a keyword
    /// and a name: the name wins, and the keyword is the fallback.
    #[must_use]
    pub const fn or_pos(self, other: Pos) -> Pos {
        if self.line == 0 {
            other
        } else {
            self
        }
    }
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warning => "warning",
        }
    }
}

/// A single diagnostic message.
#[derive(Clone, Debug)]
pub struct Diag {
    pub level: Level,
    pub file: Option<PathBuf>,
    pub pos: Pos,
    /// Length of the underlined span on the source line, in characters.
    pub span: u32,
    pub message: String,
    pub notes: Vec<String>,
    /// The text of the offending source line, when available.
    pub snippet: Option<String>,
}

impl Diag {
    pub fn error(message: impl Into<String>) -> Self {
        Diag {
            level: Level::Error,
            file: None,
            pos: Pos::default(),
            span: 1,
            message: message.into(),
            notes: Vec::new(),
            snippet: None,
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Diag {
            level: Level::Warning,
            ..Diag::error(message)
        }
    }

    pub fn at(mut self, file: impl Into<PathBuf>, pos: Pos, snippet: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self.pos = pos;
        self.snippet = Some(snippet.into());
        self
    }

    pub fn span(mut self, span: u32) -> Self {
        self.span = span.max(1);
        self
    }

    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

/// A source file plus its precomputed line index.
#[derive(Clone, Debug)]
pub struct Source {
    pub path: PathBuf,
    pub text: String,
    line_starts: Vec<usize>,
}

impl Source {
    pub fn new(path: impl Into<PathBuf>, text: impl Into<String>) -> Self {
        let text = text.into();
        let mut line_starts = vec![0usize];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Source {
            path: path.into(),
            text,
            line_starts,
        }
    }

    pub fn line_text(&self, line: u32) -> &str {
        let idx = line.saturating_sub(1) as usize;
        let Some(&start) = self.line_starts.get(idx) else {
            return "";
        };
        let end = self
            .line_starts
            .get(idx + 1)
            .copied()
            .unwrap_or(self.text.len());
        self.text[start..end].trim_end_matches(['\n', '\r'])
    }

    /// Build an error diagnostic anchored at `pos`, including the source line.
    pub fn error(&self, pos: Pos, message: impl Into<String>) -> Diag {
        Diag::error(message).at(self.path.clone(), pos, self.line_text(pos.line))
    }

    pub fn warning(&self, pos: Pos, message: impl Into<String>) -> Diag {
        Diag::warning(message).at(self.path.clone(), pos, self.line_text(pos.line))
    }

    pub fn display_path(&self) -> String {
        self.path.display().to_string().replace('\\', "/")
    }
}

/// One or more diagnostics. Compilation always stops at the first batch of
/// errors; warnings travel alongside a successful result.
#[derive(Debug)]
pub struct Error {
    pub diags: Vec<Diag>,
}

impl Error {
    pub fn new(diag: Diag) -> Self {
        Error { diags: vec![diag] }
    }
    pub fn msg(message: impl Into<String>) -> Self {
        Error::new(Diag::error(message))
    }

    /// Attach an extra note to the first diagnostic.
    pub fn note(mut self, note: impl Into<String>) -> Self {
        if let Some(first) = self.diags.first_mut() {
            first.notes.push(note.into());
        }
        self
    }

    /// Attach a note only when `cond` holds.
    pub fn note_if(self, cond: bool, note: impl Into<String>) -> Self {
        if cond {
            self.note(note)
        } else {
            self
        }
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for (i, d) in self.diags.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&render_diag(d));
        }
        out
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

impl std::error::Error for Error {}

impl From<Diag> for Error {
    fn from(d: Diag) -> Self {
        Error::new(d)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::msg(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Render one diagnostic the way `rustc` does.
pub fn render_diag(d: &Diag) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}: {}\n", d.level.label(), d.message));

    let gutter_width = d.pos.line.to_string().len().max(1);
    match (&d.file, &d.snippet) {
        (Some(path), Some(line)) => {
            out.push_str(&format!(
                "{:width$}--> {}:{}:{}\n",
                "",
                path.display().to_string().replace('\\', "/"),
                d.pos.line,
                d.pos.col,
                width = gutter_width + 1
            ));
            out.push_str(&format!("{:width$} |\n", "", width = gutter_width + 1));
            out.push_str(&format!("{} | {}\n", d.pos.line, line));
            let pad: String =
                std::iter::repeat_n(' ', d.pos.col.saturating_sub(1) as usize).collect();
            let carets: String = std::iter::repeat_n('^', d.span.max(1) as usize).collect();
            out.push_str(&format!(
                "{:width$} | {}{}\n",
                "",
                pad,
                carets,
                width = gutter_width + 1
            ));
        }
        (Some(path), None) => {
            out.push_str(&format!(
                "{:width$}--> {}:{}\n",
                "",
                path.display().to_string().replace('\\', "/"),
                d.pos,
                width = gutter_width + 1
            ));
        }
        _ => {}
    }

    for note in &d.notes {
        out.push_str(&format!(
            "{:width$} = note: {}\n",
            "",
            note,
            width = gutter_width + 1
        ));
    }
    out
}
