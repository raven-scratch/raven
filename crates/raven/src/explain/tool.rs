//! The command line, as a page.
//!
//! Built rather than written as a literal, because the manifest name and the
//! output directory are declared in [`crate::identity`] and a page that retyped
//! them could disagree with the tool.

use crate::identity;

/// `cli` — every command, every flag, and every file it writes.
pub fn cli() -> String {
    let name = identity::CRATE;
    let manifest = identity::MANIFEST;
    let out = identity::DEFAULT_OUTPUT_DIR;
    format!(
        "\n## cli\n\
         # Every command takes -m/--manifest, defaulting to {manifest} in the current\n\
         # directory, except where noted. The manifest has the same shape as\n\
         # raven-asm's: a [project] section with name/output/extensions and a [targets]\n\
         # section naming the stage and the sprites.\n\
         #\n\
         #   {name} new <name>       scaffold a project: --here (into the current\n\
         #                           directory), --force, --with-module\n\
         #   {name} init [path]      scaffold into an existing directory: --force\n\
         #   {name} check            lex, parse, resolve, type check, expand; writes nothing\n\
         #   {name} expand           print the raven-asm the whole project lowers to\n\
         #   {name} build            write {out}/<name>.sb3; --debug, --no-optimize\n\
         #   {name} fmt [paths]      canonical indentation and blank lines; --check\n\
         #   {name} clean            remove the output directory\n\
         #   {name} explain [page…]  this manual: a page, a group, or `all`\n\
         #\n\
         # # What each command writes\n\
         #\n\
         #   build           {out}/<name>.sb3. A plain build also takes back the\n\
         #                   {out}/asm/ tree a previous --debug left behind, so it\n\
         #                   cannot go stale; {out}/project.json is not a file it\n\
         #                   manages and stays until it is deleted.\n\
         #   build --debug   also {out}/asm/ — the raven-asm program raven lowered to,\n\
         #                   with its own raven-asm.toml, src/**/*.rasm and relative\n\
         #                   asset paths, so `cd {out}/asm && raven-asm build` builds\n\
         #                   the same project without raven — and {out}/project.json,\n\
         #                   the uncompressed Scratch project.\n\
         #   anything else   nothing at all.\n\
         #\n\
         # --debug is also what writes {out}/layout.json, where every list and arena\n\
         # cell landed by name. A node-side check reads it to find `list(\"board\")`\n\
         # without guessing a handle.\n\
         #\n\
         # # What check reports\n\
         #\n\
         # syntax errors with a line, a column and a caret; unresolved names with the\n\
         # closest match when there is one; type errors naming both the expected and the\n\
         # found type; macro errors (a wrong argument kind, a wrong argument type, a\n\
         # substitution that would be evaluated twice, an expansion cycle); and shape\n\
         # errors (a value where a name is wanted, a name where a value is wanted).\n\
         # check never writes and never panics: a malformed file is a diagnostic, not a\n\
         # stack trace. See the `errors` page for the format and the classes.\n\
         #\n\
         # # What expand shows\n\
         #\n\
         # The raven-asm source the project lowered to, one section per target file —\n\
         # the same text build --debug writes. A `for`, a `match`, an f-string and a\n\
         # `let` are all visible in it, because by then there is no such thing as any of\n\
         # them. It is the fastest way to answer \"what did that cost?\".\n\
         #\n\
         # # fmt\n\
         #\n\
         # Re-indents from the brace depth, trims trailing whitespace and collapses runs\n\
         # of blank lines. It is a line formatter, not a pretty-printer: it never\n\
         # reflows code and never drops a comment, because comments are not part of the\n\
         # syntax and a formatter that rebuilt the file from the syntax tree would\n\
         # delete every one of them. With no paths it formats the project the current\n\
         # directory's manifest describes; paths may be given from anywhere.\n\
         #\n\
         # # Exit codes\n\
         #\n\
         #   0   success\n\
         #   1   a diagnostic; the message says which\n\
         #   2   bad command line usage, from the argument parser\n\
         #\n\
         # A panic with a Rust backtrace is always a bug in the compiler.\n\
         #\n\
         # # The layer below has its own command line\n\
         #\n\
         # {name}-asm build|check|catalog|new|init|clean, and {name}-opt for the\n\
         # optimiser. They share the manifest shape, the error format and the {out}/\n\
         # layout, so learning one tool's flags is learning the other's. See the\n\
         # `asm-cli` page.\n\
         #\n\
         # docs: {docs}\n",
        docs = identity::DOCS,
    )
}
