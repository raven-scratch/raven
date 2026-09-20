//! Every name the *workspace* answers to, in one place.
//!
//! Per-crate identity — the crate name, the binary name, the manifest filename,
//! the source extension — lives in that crate's own `identity` module, next to
//! the code that reads it. What lives here is the part every crate must agree
//! on: the organisation, the repository and the published documentation root.
//!
//! Renaming the project is therefore: these constants, the two `identity`
//! modules in `raven-asm` and `raven`, and `docs/.vitepress/config.mts`, which
//! holds the equivalent single place for the website.

/// GitHub organisation that owns the project.
pub const ORG: &str = "raven-scratch";

/// The repository the source lives in. This is also the repository name, and
/// the VitePress `base` is `/<repository>/`.
pub const REPOSITORY_NAME: &str = "raven";

/// Fully qualified repository URL.
pub const REPOSITORY: &str = "https://github.com/raven-scratch/raven";

/// Where the combined guide for both languages is published. The trailing
/// slash matters: VitePress builds absolute links from it.
pub const DOCS: &str = "https://raven-scratch.github.io/raven/";

/// The Scratch version generation both languages target.
pub const SCRATCH_SEMVER: &str = "3.0.0";

/// The Scratch VM version recorded in a generated project's `meta.vm`.
pub const SCRATCH_VM: &str = "0.2.0";
