//! Running a build: loading, compiling, emitting raven-asm, and calling
//! `raven-asm` to produce the `.sb3`.
//!
//! raven lowers to raven-asm *source*, and this module is where that source
//! becomes real. It writes a complete raven-asm project into a staging directory
//! and hands it to [`raven_asm::compile::build`], which is the same code path
//! `raven-asm build` uses — so there is exactly one Scratch code generator in the
//! workspace, and raven cannot drift from it.
//!
//! The staging directory is `<output>/asm/`. Asset paths in the emitted source
//! are relative to it, so `--emit-asm` produces a tree that can be built on its
//! own with `raven-asm build`. Without the flag the files this build wrote are
//! removed again; the directory itself, and anything else in it, is left alone.

use std::path::{Path, PathBuf};

use raven_scratch::diag::{Diag, Error, Result};

use crate::identity;
use crate::lower;
use crate::module::{self, Program};
use crate::rasm;

/// What a build was asked to do.
#[derive(Clone, Debug)]
pub struct Options {
    /// The `raven.toml` to build, or the directory holding one.
    pub manifest: PathBuf,
    /// Debug output: keep the emitted raven-asm project in `<output>/asm/` and
    /// write the uncompressed `<output>/project.json`.
    pub debug: bool,
    /// Keep the emitted raven-asm without also writing `project.json`.
    /// `--debug` implies this; it exists so the two halves can be tested apart.
    pub emit_asm: bool,
}

impl Options {
    #[must_use]
    pub fn new(manifest: impl Into<PathBuf>) -> Self {
        Self {
            manifest: manifest.into(),
            emit_asm: false,
            debug: false,
        }
    }
}

/// What a successful build produced.
#[derive(Debug, Default)]
pub struct BuildResult {
    /// The generated raven-asm, as (path relative to the staging root, source).
    pub asm: Vec<(PathBuf, String)>,
    /// Where the staging tree lives, when it was kept.
    pub asm_dir: Option<PathBuf>,
    /// The `.sb3`.
    pub artifact: Option<PathBuf>,
    pub warnings: Vec<Diag>,
}

/// Load and compile, without writing anything.
pub fn check(options: &Options) -> Result<Vec<Diag>> {
    let program = module::load(&options.manifest)?;
    let output = lower::compile(&program)?;
    let mut warnings = output.warnings.clone();
    warnings.extend(unused_extensions(&program, &output));
    Ok(warnings)
}

/// Print the raven-asm the project lowers to.
pub fn expand(options: &Options) -> Result<Vec<(PathBuf, String)>> {
    let program = module::load(&options.manifest)?;
    let output = lower::compile(&program)?;
    Ok(files(&program, &output))
}

/// Compile to a `.sb3`.
pub fn build(options: &Options) -> Result<BuildResult> {
    let program = module::load(&options.manifest)?;
    let output = lower::compile(&program)?;
    let mut result = BuildResult {
        warnings: output.warnings.clone(),
        ..BuildResult::default()
    };
    result.warnings.extend(unused_extensions(&program, &output));

    let target_path = output_path(&program, options)?;
    let asm_dir = target_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("asm");
    let out = files(&program, &output);
    result.asm = out.clone();

    // `--debug` is the one switch that turns a build into an inspection: it
    // keeps the raven-asm this project lowered to and writes the uncompressed
    // `project.json` beside the archive. A normal build is one `.sb3` and
    // nothing else, so `dist/` never contains something the user did not ask
    // for and did not know was stale.
    let keep_asm = options.debug || options.emit_asm;

    let staging = staging_manifest(&program, &target_path, &asm_dir, &out);
    let written = write_tree(&asm_dir, &out, &staging)?;

    let manifest_path = asm_dir.join(raven_asm::manifest::MANIFEST_NAME);
    let build = raven_asm::compile::build(&manifest_path)?;
    result.warnings.extend(build.warnings.clone());

    // `compile::build` produces the project and its assets; writing the archive
    // is the caller's job, and this is the same code `raven-asm build` runs.
    let json = serde_json::to_vec(&build.project)
        .map_err(|e| Error::msg(format!("cannot serialize project.json: {e}")))?;
    let mut zip = raven_scratch::zipw::ZipWriter::new();
    zip.add("project.json", json.clone());
    for asset in &build.assets {
        zip.add(asset.filename(), asset.data.clone());
    }
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target_path, zip.finish())?;
    result.artifact = Some(target_path.clone());

    if keep_asm {
        result.asm_dir = Some(asm_dir.clone());
        if options.debug {
            let pretty = serde_json::to_string_pretty(&build.project)
                .map_err(|e| Error::msg(format!("cannot serialize project.json: {e}")))?;
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent)?;
                std::fs::write(parent.join("project.json"), pretty)?;
            }
        }
    } else {
        // Take back what this build wrote, and the directory too when nothing
        // else is in it: a plain build leaves no staging tree behind.
        for path in written {
            let _ = std::fs::remove_file(path);
        }
        remove_empty_dirs(&asm_dir)?;
    }
    Ok(result)
}

/// Remove `dir` and any subdirectories that are now empty.
///
/// A file this build did not write — the user's own note in `dist/asm/` — keeps
/// its directory alive; a staging tree nobody asked to keep is removed whole.
fn remove_empty_dirs(dir: &Path) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            remove_empty_dirs(&path)?;
        }
    }
    // `remove_dir` refuses a directory that still has anything in it, which is
    // exactly the test we want.
    let _ = std::fs::remove_dir(dir);
    Ok(())
}

/// The absolute path of the `.sb3` the project writes.
fn output_path(program: &Program, options: &Options) -> Result<PathBuf> {
    let _ = options;
    let root = &program.root;
    let raw = program
        .manifest
        .project
        .output
        .as_deref()
        .unwrap_or(identity::DEFAULT_OUTPUT_DIR);
    let joined = root.join(raw);
    if raw.ends_with(".sb3") {
        Ok(joined)
    } else {
        Ok(joined.join(format!("{}.sb3", sanitis(&program.manifest.project.name))))
    }
}
fn sanitis(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.trim_matches('-').is_empty() {
        "project".to_string()
    } else {
        cleaned.trim_matches('-').to_string()
    }
}

/// The raven-asm file each target lowers to.
fn files(program: &Program, output: &lower::Output) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    for (index, plan) in program.targets.iter().enumerate() {
        let Some(file) = output.files.get(index) else {
            continue;
        };
        let path = if plan.kind == crate::ast::TargetKind::Stage {
            PathBuf::from("src/stage.rasm")
        } else {
            PathBuf::from("src/sprites").join(format!("{}.rasm", sanitis(&plan.name)))
        };
        out.push((path, rasm::print(file)));
    }
    out
}

/// The manifest raven-asm reads to build the staged project.
fn staging_manifest(
    program: &Program,
    target_path: &Path,
    asm_dir: &Path,
    files: &[(PathBuf, String)],
) -> String {
    let stage = files
        .first()
        .map_or_else(|| "src/stage.rasm".to_string(), |(p, _)| slash(p));
    let sprites: Vec<String> = files
        .iter()
        .skip(1)
        .map(|(p, _)| format!("    {},", rasm::quote(&slash(p))))
        .collect();
    let extensions: Vec<String> = program
        .manifest
        .project
        .extensions
        .iter()
        .map(|e| rasm::quote(e))
        .collect();
    let output = relative_dir(asm_dir, target_path.parent().unwrap_or(Path::new(".")));
    let name = target_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("project");
    let mut out = String::new();
    out.push_str("[project]\n");
    out.push_str(&format!("name = {}\n", rasm::quote(name)));
    out.push_str(&format!("output = {}\n", rasm::quote(&output)));
    out.push_str(&format!("extensions = [{}]\n", extensions.join(", ")));
    out.push_str("\n[targets]\n");
    out.push_str(&format!("stage = {}\n", rasm::quote(&stage)));
    if sprites.is_empty() {
        out.push_str("sprites = []\n");
    } else {
        out.push_str("sprites = [\n");
        for sprite in sprites {
            out.push_str(&sprite);
            out.push('\n');
        }
        out.push_str("]\n");
    }
    out
}

fn slash(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// The path from `from` to `to`, both of which exist on disk.
fn relative_dir(from: &Path, to: &Path) -> String {
    let from = from.canonicalize().unwrap_or_else(|_| from.to_path_buf());
    let to = to.canonicalize().unwrap_or_else(|_| to.to_path_buf());
    let mut a = from.components().peekable();
    let mut b = to.components().peekable();
    while a.peek().is_some() && a.peek() == b.peek() {
        a.next();
        b.next();
    }
    let mut out = PathBuf::new();
    for _ in a {
        out.push("..");
    }
    for component in b {
        out.push(component);
    }
    if out.as_os_str().is_empty() {
        ".".to_string()
    } else {
        slash(&out)
    }
}

/// Write the emitted tree without destroying anything else in `dir`.
///
/// A stale `.rasm` file from a previous build is removed; every other file —
/// including one the user put there — is left alone, and the returned paths are
/// what this build wrote, so `build` can take them back when `--emit-asm` is
/// absent.
fn write_tree(dir: &Path, files: &[(PathBuf, String)], manifest: &str) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let generated: Vec<PathBuf> = files.iter().map(|(path, _)| dir.join(path)).collect();
    remove_stale_rasm(dir, &generated)?;
    let manifest_path = dir.join(raven_asm::manifest::MANIFEST_NAME);
    std::fs::write(&manifest_path, manifest)?;
    let mut written = vec![manifest_path];
    for (path, source) in files {
        let full = dir.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full, source)?;
        written.push(full);
    }
    Ok(written)
}

/// Remove `.rasm` files under `dir` that this build did not generate.
fn remove_stale_rasm(dir: &Path, generated: &[PathBuf]) -> Result<()> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rasm")
                && !generated.contains(&path)
            {
                std::fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}

/// A warning for an extension the project asks for and never uses.
fn unused_extensions(program: &Program, output: &lower::Output) -> Vec<Diag> {
    let mut used: Vec<&str> = Vec::new();
    for file in &output.files {
        let Some(target) = &file.target else { continue };
        collect_extensions(&target.items, &mut used);
    }
    program
        .manifest
        .project
        .extensions
        .iter()
        .filter(|name| !used.contains(&name.as_str()))
        .map(|name| {
            Diag::warning(format!(
                "the `{name}` extension is enabled in the manifest but no block uses it"
            ))
        })
        .collect()
}

fn collect_extensions(items: &[raven_asm::ast::Item], used: &mut Vec<&'static str>) {
    for item in items {
        match item {
            raven_asm::ast::Item::Stmt(stmt) => collect_stmt_extensions(stmt, used),
            raven_asm::ast::Item::Proc(proc) => {
                for stmt in &proc.body {
                    collect_stmt_extensions(stmt, used);
                }
            }
            _ => {}
        }
    }
}

fn collect_stmt_extensions(stmt: &raven_asm::ast::Stmt, used: &mut Vec<&'static str>) {
    if let Some(extension) = extension_of(&stmt.opcode) {
        used.push(extension);
    }
    for body in [&stmt.body, &stmt.else_body].into_iter().flatten() {
        for stmt in body {
            collect_stmt_extensions(stmt, used);
        }
    }
}

fn extension_of(opcode: &str) -> Option<&'static str> {
    raven_scratch::catalog::block(opcode).and_then(|spec| spec.category.extension())
}

/// The manifest a project is loaded from, resolved from a directory or a file.
#[must_use]
pub fn manifest_path(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join(identity::MANIFEST)
    } else {
        path.to_path_buf()
    }
}

/// The path of the `.sb3` a project would write, for `raven clean` and messages.
pub fn artifact_of(program: &Program) -> PathBuf {
    let options = Options::new(PathBuf::new());
    output_path(program, &options).unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_sanitised_for_the_file_system() {
        assert_eq!(sanitis("hello"), "hello");
        assert_eq!(sanitis("my project"), "my-project");
        assert_eq!(sanitis("///"), "project");
        assert_eq!(sanitis("a_b-c"), "a_b-c");
    }
}
