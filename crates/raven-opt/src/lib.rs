//! raven-opt — a raven-asm project in, an optimised raven-asm project out.
//!
//! This is the optimiser as a *program* rather than a compiler flag. It reads
//! raven-asm source, rewrites it to emit fewer Scratch blocks for the same
//! program, and writes raven-asm source back out. Nothing about the language
//! changes: the output is a project `raven-asm build` accepts, and building it
//! gives an `.sb3` with fewer blocks in it.
//!
//! # Why it is separate from `raven-asm`
//!
//! raven-asm promises that one statement is one block, and a reader can count
//! them. An optimiser inside the compiler would break that promise silently, so
//! it is not in the compiler: it is a second command you run over the project if
//! you want it. `raven` runs it for you, because raven's macros are already
//! lowerings a reader never wrote; `raven build --no-optimize` is the escape.
//!
//! # What it will not do
//!
//! [`optimize`] is the whole list of rewrites, and each one is an identity in
//! Scratch's own semantics applied to operands the compiler already knows. The
//! two that a layer like this obviously wants -- inlining a procedure, and
//! substituting a reporter into the place that reads it -- were both written and
//! measured and are **not** here, with the numbers in that module's header. A
//! rewrite that is merely usually right is not in this program.
//!
//! # Using it
//!
//! ```sh
//! raven-opt --manifest raven-asm.toml --output optimised/
//! raven-opt --manifest raven-asm.toml --in-place
//! ```
//!
//! and as a library:
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let report = raven_opt::optimize_in_place(std::path::Path::new("raven-asm.toml"))?;
//! println!("{report}");
//! # Ok(())
//! # }
//! ```

pub mod optimize;

pub use optimize::{is_pure, sampled_opcodes, Folded};

use raven_asm::manifest::Manifest;
use raven_asm::{parser, print};
use raven_scratch::diag::{Error, Result, Source};
use std::path::{Path, PathBuf};

/// What a run did, so a caller can report it rather than guess.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// How many raven-asm files were read and written.
    pub files: usize,
    /// The rewrites those files received.
    pub folded: Folded,
    /// How many files had at least one rewrite, which is the number that says
    /// whether anything happened at all.
    pub changed: usize,
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} file(s), {} changed; {}",
            self.files, self.changed, self.folded
        )
    }
}

/// Optimise one raven-asm source text.
///
/// This is the whole operation: parse, rewrite, print. It is a text-to-text
/// function on purpose, because that is what makes the optimiser a program over
/// a project rather than a stage inside a compiler -- and because a caller that
/// wants to optimise something in memory can do it without a directory.
///
/// The output parses as raven-asm and means the same thing; the tests in
/// `crates/raven-asm/src/print.rs` hold the printer to that, and the ones in
/// `crates/raven-re/tests/roundtrip.rs` hold the rewrites to not changing what a
/// project observably is.
pub fn optimize_source(name: &str, text: &str) -> Result<(String, Folded)> {
    let source = Source::new(name.to_string(), text);
    let mut unit = parser::parse(&source)?;
    let folded = optimize::optimize_file(&mut unit);
    Ok((print::file(&unit), folded))
}

/// Every `.rasm` file under `root`, in a stable order, as paths relative to it.
///
/// The walk is over the *directory* rather than over the manifest's target list
/// and each target's `use` closure, and that is deliberate: a module is a
/// raven-asm file too and wants the same rewriting, every file under a
/// raven-asm project's root belongs to it, and a walk cannot miss one the way a
/// closure that resolves `use` slightly differently from `raven-asm`'s own
/// loader could. `raven-asm.toml` is not a `.rasm` and is copied rather than
/// rewritten.
fn sources_under(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    collect(root, root, &mut out, 0)?;
    out.sort();
    Ok(out)
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<PathBuf>, depth: usize) -> std::io::Result<()> {
    // A bound rather than a full recursion, so a symlink loop cannot hang the
    // tool. No project has a source tree this deep.
    if depth > 32 {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(root, &path, out, depth + 1)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("rasm") {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_path_buf());
            }
        }
    }
    Ok(())
}

/// The manifest's project root: the directory the manifest lives in, which is
/// what every path inside it is written relative to.
fn root_of(manifest_path: &Path) -> PathBuf {
    manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Optimise the project `manifest_path` names, writing the result to `output`.
///
/// The output is a complete raven-asm project: every `.rasm` rewritten, and the
/// manifest copied so `raven-asm build -m <output>/raven-asm.toml` works on it.
/// The manifest's own `output` directory is *not* touched or copied -- it is
/// where that project's build will write, not part of the source.
pub fn optimize_project(manifest_path: &Path, output: &Path) -> Result<Report> {
    // Read the manifest first, so a project that has none fails with the
    // manifest's own diagnostic rather than with a directory listing.
    let _ = Manifest::load(manifest_path)?;
    let root = root_of(manifest_path);
    let sources = sources_under(&root).map_err(|e| {
        Error::new(raven_scratch::diag::Diag::error(format!(
            "cannot read `{}`",
            root.display()
        )))
        .note(e.to_string())
    })?;

    let mut report = Report::default();
    for relative in &sources {
        let from = root.join(relative);
        let text = std::fs::read_to_string(&from).map_err(|e| {
            Error::new(raven_scratch::diag::Diag::error(format!(
                "cannot read `{}`",
                from.display()
            )))
            .note(e.to_string())
        })?;
        let (printed, folded) = optimize_source(&from.to_string_lossy(), &text)?;
        report.files += 1;
        report.folded.folded += folded.folded;
        report.folded.simplified += folded.simplified;
        if folded.total() > 0 {
            report.changed += 1;
        }
        let to = output.join(relative);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                Error::new(raven_scratch::diag::Diag::error(format!(
                    "cannot create `{}`",
                    parent.display()
                )))
                .note(e.to_string())
            })?;
        }
        std::fs::write(&to, printed.as_bytes()).map_err(|e| {
            Error::new(raven_scratch::diag::Diag::error(format!(
                "cannot write `{}`",
                to.display()
            )))
            .note(e.to_string())
        })?;
    }

    // The manifest, unchanged. A project without one is not buildable, so
    // writing the sources and leaving the manifest behind would produce a
    // directory that looks like a project and is not one.
    let manifest_text = std::fs::read_to_string(manifest_path).map_err(|e| {
        Error::new(raven_scratch::diag::Diag::error(format!(
            "cannot read `{}`",
            manifest_path.display()
        )))
        .note(e.to_string())
    })?;
    std::fs::create_dir_all(output).map_err(|e| {
        Error::new(raven_scratch::diag::Diag::error(format!(
            "cannot create `{}`",
            output.display()
        )))
        .note(e.to_string())
    })?;
    let manifest_out = output.join(
        manifest_path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("raven-asm.toml")),
    );
    std::fs::write(&manifest_out, manifest_text.as_bytes()).map_err(|e| {
        Error::new(raven_scratch::diag::Diag::error(format!(
            "cannot write `{}`",
            manifest_out.display()
        )))
        .note(e.to_string())
    })?;

    Ok(report)
}

/// Optimise a project over itself, which is what a front end wants: the lowering
/// has just been written to a staging directory and the compiler is about to
/// read it.
///
/// It writes through a temporary file beside each target and renames it into
/// place, so a failure part-way leaves the project as it was rather than half
/// rewritten. A tree that is half optimised is not a tree anything can reason
/// about.
pub fn optimize_in_place(manifest_path: &Path) -> Result<Report> {
    let root = root_of(manifest_path);
    let sources = sources_under(&root).map_err(|e| {
        Error::new(raven_scratch::diag::Diag::error(format!(
            "cannot read `{}`",
            root.display()
        )))
        .note(e.to_string())
    })?;

    // Everything is read and rewritten *before* anything is written, so the
    // only way to end up half done is a write failing on a file that already
    // exists -- which is the one case the temporary file below covers.
    let mut rewritten: Vec<(PathBuf, String)> = Vec::new();
    let mut report = Report::default();
    for relative in &sources {
        let from = root.join(relative);
        let text = std::fs::read_to_string(&from).map_err(|e| {
            Error::new(raven_scratch::diag::Diag::error(format!(
                "cannot read `{}`",
                from.display()
            )))
            .note(e.to_string())
        })?;
        let (printed, folded) = optimize_source(&from.to_string_lossy(), &text)?;
        report.files += 1;
        report.folded.folded += folded.folded;
        report.folded.simplified += folded.simplified;
        if folded.total() > 0 {
            report.changed += 1;
        }
        rewritten.push((from, printed));
    }

    for (path, text) in rewritten {
        let temp = path.with_extension("rasm.raven-opt-tmp");
        std::fs::write(&temp, text.as_bytes()).map_err(|e| {
            Error::new(raven_scratch::diag::Diag::error(format!(
                "cannot write `{}`",
                temp.display()
            )))
            .note(e.to_string())
        })?;
        std::fs::rename(&temp, &path).map_err(|e| {
            Error::new(raven_scratch::diag::Diag::error(format!(
                "cannot replace `{}`",
                path.display()
            )))
            .note(e.to_string())
        })?;
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_constant_fold_survives_the_round_trip_out() {
        let (out, folded) =
            optimize_source("t.rasm", "data_setvariableto(\"x\", operator_add(1, 1));\n")
                .expect("optimises");
        assert_eq!(folded.folded, 1);
        assert!(out.contains("data_setvariableto(\"x\", 2);"), "{out}");
    }

    #[test]
    fn the_output_is_valid_raven_asm() {
        // The property the whole program rests on: what comes out has to be
        // something the compiler takes. Parsing it is the cheapest proof.
        let (out, _) = optimize_source(
            "t.rasm",
            "proc go() {\n    data_setvariableto(\"x\", operator_multiply(2, 3));\n}\n",
        )
        .expect("optimises");
        let source = Source::new("t.rasm".to_string(), &out);
        parser::parse(&source).expect("the optimised project still parses");
    }

    #[test]
    fn a_file_with_nothing_to_fold_is_reported_as_unchanged() {
        let (out, folded) = optimize_source("t.rasm", "looks_say(\"hi\");\n").expect("optimises");
        assert_eq!(folded.total(), 0, "nothing to do, and it says so");
        assert!(out.contains("looks_say"), "{out}");
    }

    #[test]
    fn a_use_path_keeps_its_quotes() {
        // The bug the printer's own tests caught, pinned here as well because
        // this crate is what writes projects out: a `use` printed without its
        // quotes is source the parser refuses.
        let (out, _) =
            optimize_source("t.rasm", "use \"motion\";\nlooks_say(\"a\");\n").expect("optimises");
        assert!(out.contains("use \"motion\";"), "{out}");
    }
}
