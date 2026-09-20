//! The Scratch 3 domain model shared by every raven language front end.
//!
//! Nothing in this crate knows about a *source* language. It is the target:
//! the block vocabulary ([`catalog`]), the `.sb3` container ([`sb3`], [`zipw`]),
//! deterministic identifier generation ([`ids`]), asset hashing and measurement
//! ([`assets`]), shared source positions and diagnostics ([`diag`]), and the
//! names this workspace answers to ([`identity`]).
//!
//! The split exists so that `raven-asm` — which compiles to Scratch — and
//! `raven` — which compiles to `raven-asm` — can agree on what a Scratch block
//! is without either one owning the other. `raven-asm` depends on this crate;
//! `raven` depends on this crate and on `raven-asm`, never the other way round.

pub mod assets;
pub mod catalog;
pub mod diag;
pub mod identity;
pub mod ids;
pub mod sb3;
pub mod zipw;
