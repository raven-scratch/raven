//! raven-asm — a line-by-line Scratch 3 compiler.
//!
//! raven-asm source maps one statement to exactly one Scratch block. There is no
//! desugaring, no macro system and no operator overloading: what you write is
//! what the Scratch editor shows.
//!
//! The workspace splits the work in two. Everything that describes *Scratch* —
//! the block catalog, the `.sb3` container, deterministic ids, asset handling
//! and diagnostics — lives in the [`raven_scratch`] crate, because the `raven`
//! front end needs exactly the same vocabulary. This crate owns the *source*
//! language and the compiler that lowers it:
//!
//! * [`lexer`] / [`parser`] / [`ast`] — reading raven-asm source.
//! * [`source`] — writing it: the escapes and spellings the lexer accepts.
//! * [`compile`] — source files to a [`raven_scratch::sb3::Project`].
//! * [`manifest`] / [`scaffold`] — `raven-asm.toml` and `raven-asm new`.
//! * [`cli`] / [`docs_gen`] — the command line front end and the generated
//!   block reference.
//!
//! The Scratch-side modules are re-exported here so that code inside this crate
//! can keep reading `crate::catalog` while the layering stays visible at the
//! crate boundary: this crate depends on [`raven_scratch`], never the reverse.

pub mod ast;
pub mod cli;
pub mod compile;
pub mod docs_gen;
pub mod identity;
pub mod lexer;
pub mod manifest;
pub mod parser;
pub mod scaffold;
pub mod source;

pub use raven_scratch::{assets, catalog, diag, ids, sb3, zipw};
