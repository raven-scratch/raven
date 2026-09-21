//! Every name the `raven-re` tool answers to, in one place.
//!
//! The decompiler writes a raven-asm project, so the manifest filename, the
//! source extension and the layout it writes are raven-asm's, and are read from
//! that crate rather than restated here.

/// Crate and binary name. Must match `name` / `[[bin]] name` in `Cargo.toml`.
pub const CRATE: &str = "raven-re";

/// Human-readable name, as it appears in help text and in generated files.
pub const DISPLAY: &str = "raven-re";

/// The manifest file of the project that is written.
pub const MANIFEST: &str = raven_asm::identity::MANIFEST;

/// File extension of the source files that are written.
pub const SOURCE_EXTENSION: &str = raven_asm::identity::SOURCE_EXTENSION;

/// Directory the stage and the sprite files go in.
pub const SOURCE_DIR: &str = "src";

/// Directory the sprite files go in, below [`SOURCE_DIR`].
pub const SPRITE_DIR: &str = "src/sprites";

/// The stage file, relative to the project root.
pub const STAGE_FILE: &str = "src/stage.rasm";

/// Directory the costumes and sounds go in.
pub const ASSET_DIR: &str = "assets";

/// Where the project lives. Shared with every other crate in the workspace.
pub const REPOSITORY: &str = raven_scratch::identity::REPOSITORY;

/// Where the guide is published. Shared with every other crate in the workspace.
pub const DOCS: &str = raven_scratch::identity::DOCS;
