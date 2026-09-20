//! Every name the `raven-asm` tool answers to, in one place.
//!
//! The compiler, the CLI and the scaffold all read their identity from here, so
//! renaming the tool is an edit to this file plus `Cargo.toml` — not a
//! search-and-replace across the tree. The repository and documentation root are
//! shared with the `raven` front end and live in `raven_scratch::identity`; the
//! documentation keeps its own copy of the site title and `base` in
//! `docs/.vitepress/config.mts`, which is the equivalent single place on that
//! side.

/// Crate and binary name. Must match `name` / `[[bin]] name` in `Cargo.toml`.
pub const CRATE: &str = "raven-asm";

/// Human-readable name, as it appears in help text and in generated files.
pub const DISPLAY: &str = "raven-asm";

/// The manifest file `raven-asm build` and friends look for.
pub const MANIFEST: &str = "raven-asm.toml";

/// File extension for source files, and for `use` paths that omit one.
pub const SOURCE_EXTENSION: &str = "rasm";

/// Where the project lives. Shared with every other crate in the workspace.
pub const REPOSITORY: &str = raven_scratch::identity::REPOSITORY;

/// Where the guide is published. Shared with every other crate in the workspace.
pub const DOCS: &str = raven_scratch::identity::DOCS;

/// Directory the `.sb3` is written to when the manifest does not say.
pub const DEFAULT_OUTPUT_DIR: &str = "dist";

/// The `meta.agent` string written into every generated project.
pub fn agent() -> String {
    format!("{CRATE} {}", env!("CARGO_PKG_VERSION"))
}
