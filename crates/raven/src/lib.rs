//! raven — the high-level language.
//!
//! raven is the layer above [`raven_asm`]: it has expressions, control flow,
//! declared types, inlined functions and a macro system, and it compiles to
//! raven-asm, which compiles to a Scratch 3 `.sb3`. Every convenience it adds is
//! a macro, and `raven expand` prints the raven-asm it became.
//!
//! The language is specified in `docs/raven/`, and the specification is the
//! contract. These modules are the parts of the implementation that have to
//! agree with it:
//!
//! * [`ast`] — the surface syntax, exactly as the parser produces it. Every
//!   convenience over the core grammar is already a macro call by the time it
//!   gets here, because that is what it is.
//! * [`ty`] — `num`, `str`, `bool`, `list<T>`, and how they meet the block
//!   catalog's shapes. The two conversions raven has are both free.
//! * [`purity`] — `pure`, `sampled` or `effectful` for every catalog block. This
//!   is what makes `let` a safe substitution.
//! * [`stdlib`] — every one of the catalog's blocks, bound to the raven name that
//!   reaches it, with a test that fails the build when the binding falls behind.
//! * [`pipeline`] — the stages of the front end and their contracts.
//! * [`explain`] — the language written for a machine reader: `raven explain`.
//! * [`diag`] — diagnostics, plus the macro expansion backtrace that only raven
//!   needs.
//!
//! The two rules that shape all of it:
//!
//! > **Every expansion is printable, and expansion is total.**
//!
//! Nothing is lowered at a stage you cannot ask the CLI to print, and no macro
//! can expand forever.

pub mod ast;
pub mod cli;
pub mod diag;
pub mod driver;
pub mod explain;
pub mod fmt;
pub mod identity;
pub mod lexer;
pub mod lower;
pub mod menu;
pub mod module;
pub mod parser;
pub mod purity;
pub mod rasm;
pub mod scaffold;
pub mod stdlib;
pub mod ty;
