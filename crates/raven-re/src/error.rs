//! Errors and warnings.
//!
//! A decompiler reads bytes, not source text, so there is no span to underline:
//! a failure names the thing that is wrong and explains what raven-re can do
//! instead. The rendering matches the rest of the workspace, `error:` first and
//! `= note:` lines under it.

use std::fmt;

/// A failure that stops the reversal.
#[derive(Clone, Debug)]
pub struct Error {
    pub message: String,
    pub notes: Vec<String>,
}

impl Error {
    pub fn msg(message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
            notes: Vec::new(),
        }
    }

    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn render(&self) -> String {
        render("error", &self.message, &self.notes)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::msg(e.to_string())
    }
}

/// Something worth saying that does not stop the reversal.
#[derive(Clone, Debug)]
pub struct Warning {
    pub message: String,
    pub notes: Vec<String>,
}

impl Warning {
    pub fn new(message: impl Into<String>) -> Self {
        Warning {
            message: message.into(),
            notes: Vec::new(),
        }
    }

    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn render(&self) -> String {
        render("warning", &self.message, &self.notes)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

fn render(level: &str, message: &str, notes: &[String]) -> String {
    let mut out = format!("{level}: {message}\n");
    for note in notes {
        out.push_str(&format!("  = note: {note}\n"));
    }
    out
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}
