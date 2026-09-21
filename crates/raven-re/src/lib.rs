//! raven-re — reverse a vanilla Scratch 3 `.sb3` project into raven-asm source.
//!
//! The workspace compiles in one direction:
//!
//! ```text
//! raven (.rav) -> raven-asm (.rasm) -> project.json -> .sb3
//! ```
//!
//! raven-asm exists because one statement is exactly one Scratch block, which
//! makes that arrow reversible. This crate walks it backwards: an `.sb3` in, a
//! raven-asm project out — the same `raven-asm.toml`, `src/stage.rasm`,
//! `src/sprites/*.rasm` and `assets/` that `raven-asm build` reads, so
//! `raven-asm check` validates the result and `raven-asm build` rebuilds it.
//!
//! The reversal is exact where raven-asm has syntax and explicit where it does
//! not:
//!
//! * a project that is not vanilla Scratch 3 is refused, naming the block,
//!   extension or agent that gave it away — TurboWarp and any other edit of
//!   Scratch have no raven-asm spelling;
//! * a name an identifier cannot hold is encoded deterministically, so the same
//!   file always reverses to the same source;
//! * what has no syntax at all — Scratch comments, costume `bitmapResolution`,
//!   monitors that watch a reporter — is reported as a warning and dropped.
//!
//! This is a decompiler, not a de-optimiser: it never guesses at a structure the
//! project does not have, and it never invents a macro.

pub mod cli;
pub mod error;
pub mod identity;
pub mod names;
pub mod reverse;
pub mod zipr;

pub use error::{Error, Result, Warning};
pub use reverse::{decompile, Decompiled};
