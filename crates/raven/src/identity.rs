//! Every name the `raven` tool answers to, in one place.
//!
//! The front end, the CLI and the scaffold all read their identity from here.
//! The repository and documentation root are shared with the other crates and
//! live in `raven_scratch::identity`.

/// Crate and binary name. Must match `name` / `[[bin]] name` in `Cargo.toml`.
pub const CRATE: &str = "raven";

/// Human-readable name, as it appears in help text and in generated files.
pub const DISPLAY: &str = "raven";

/// The manifest file `raven build` and friends look for.
pub const MANIFEST: &str = "raven.toml";

/// File extension for source files, and for `use` paths that omit one.
pub const SOURCE_EXTENSION: &str = "rav";

/// Where the project lives. Shared with every other crate in the workspace.
pub const REPOSITORY: &str = raven_scratch::identity::REPOSITORY;

/// Where the guide is published. Shared with every other crate in the workspace.
pub const DOCS: &str = raven_scratch::identity::DOCS;

/// Directory the `.sb3` is written to when the manifest does not say.
pub const DEFAULT_OUTPUT_DIR: &str = "dist";

/// The module the prelude is imported from, and the name it answers to in the
/// diagnostics and in `raven expand` output.
pub const PRELUDE: &str = "std::prelude";

/// `meta.agent` written into a project that raven built. It names the whole
/// chain, because a raven build is a raven-asm build underneath.
pub fn agent() -> String {
    format!(
        "{CRATE} {} ({})",
        env!("CARGO_PKG_VERSION"),
        raven_asm::identity::agent()
    )
}
