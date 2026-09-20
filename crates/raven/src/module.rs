//! Reading a raven project: the manifest, the sources, and the module graph.
//!
//! The manifest is the same shape as raven-asm's, so the project model is the
//! same: one `stage` file, any number of sprite files, and module files that
//! other files `use`. Only the file name and the extension differ, and this
//! module is where that difference lives.
//!
//! Module paths are the file path under `src/`, written with `::` — so
//! `src/lib/geometry.rav` is `lib::geometry`. There are no `mod` blocks and no
//! path attributes, which means a module's identity is decided by the file
//! system and nothing else.
//!
//! A `use` path may name an item rather than a module, and the two are told
//! apart by looking:
//!
//! ```text
//! use lib::geometry;                 // the module
//! use lib::geometry::hypot;          // one item from it
//! use lib::geometry::{hypot, area};  // several
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use raven_asm::manifest::Manifest;
use raven_scratch::diag::{Diag, Error, Pos, Result, Source};

use crate::ast;
use crate::identity;
use crate::parser;

/// One parsed source file.
#[derive(Debug)]
pub struct Unit {
    /// The module path (`lib::geometry`), or the file stem for a target file.
    pub key: String,
    pub path: PathBuf,
    pub source: Rc<Source>,
    pub file: ast::File,
}

/// Everything needed to lower one target.
#[derive(Debug)]
pub struct TargetPlan {
    pub name: String,
    pub kind: ast::TargetKind,
    /// The file that declares the target.
    pub main: Rc<Unit>,
    /// Modules it uses, transitively, in import order, each once. A module that
    /// two files import appears here once, and its items are copied into each
    /// target that needs them.
    pub modules: Vec<Rc<Unit>>,
}

/// A loaded project.
#[derive(Debug)]
pub struct Program {
    pub root: PathBuf,
    pub manifest: Manifest,
    pub targets: Vec<TargetPlan>,
    /// Every file that was parsed, including module files.
    pub files: Vec<Rc<Unit>>,
}

impl Program {
    /// Every module file in the project, whether or not a target uses it.
    pub fn modules(&self) -> impl Iterator<Item = &Rc<Unit>> {
        self.files
            .iter()
            .filter(|unit| unit.file.target().is_none())
    }
}

/// Read and parse a whole project.
pub fn load(manifest_path: &Path) -> Result<Program> {
    let manifest_path = if manifest_path.is_dir() {
        manifest_path.join(identity::MANIFEST)
    } else {
        manifest_path.to_path_buf()
    };
    let manifest = Manifest::load(&manifest_path)?;
    let root = manifest_path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);

    let mut loader = Loader {
        root: root.clone(),
        cache: HashMap::new(),
        by_key: HashMap::new(),
        files: Vec::new(),
    };

    let mut targets = Vec::new();
    targets.push(loader.target(&manifest.stage_path(&root))?);
    for path in manifest.sprite_paths(&root) {
        targets.push(loader.target(&path)?);
    }

    let program = Program {
        root,
        manifest,
        targets,
        files: std::mem::take(&mut loader.files),
    };
    check_target_names(&program)?;
    Ok(program)
}

/// Sprite names must be unique, because Scratch identifies a target by name, and
/// `Stage` is the stage's name whether or not a sprite claims it.
fn check_target_names(program: &Program) -> Result<()> {
    let mut seen: Vec<&str> = Vec::new();
    for target in &program.targets {
        if target.kind == ast::TargetKind::Sprite {
            if target.name == "Stage" {
                let span = target.main.file.span;
                return Err(Error::new(
                    target
                        .main
                        .source
                        .error(span.pos, "`Stage` is the reserved name of the stage target")
                        .note("rename the sprite"),
                ));
            }
            if seen.contains(&target.name.as_str()) {
                let span = target.main.file.span;
                return Err(Error::new(target.main.source.error(
                    span.pos,
                    format!("two sprites are called `{}`", target.name),
                )));
            }
            seen.push(&target.name);
        }
    }
    Ok(())
}

struct Loader {
    root: PathBuf,
    /// Canonical path to the parsed unit.
    cache: HashMap<PathBuf, Rc<Unit>>,
    /// Module key to the parsed unit.
    by_key: HashMap<String, Rc<Unit>>,
    files: Vec<Rc<Unit>>,
}

impl Loader {
    fn target(&mut self, path: &Path) -> Result<TargetPlan> {
        let unit = self.file(path)?;
        let Some(declared) = unit.file.target() else {
            return Err(Error::new(unit.source.error(
                unit.file.span.pos,
                format!(
                    "`{}` is listed as a target but declares no `stage` or `sprite`",
                    display(path, &self.root)
                ),
            )));
        };
        let kind = declared.kind;
        let name = if kind == ast::TargetKind::Stage {
            "Stage".to_string()
        } else {
            declared.name.clone()
        };

        let mut modules = Vec::new();
        let mut stack = Vec::new();
        let uses = unit.file.uses.clone();
        for decl in &uses {
            self.resolve_uses(&unit, decl, &mut modules, &mut stack)?;
        }
        Ok(TargetPlan {
            name,
            kind,
            main: unit,
            modules,
        })
    }

    fn resolve_uses(
        &mut self,
        from: &Rc<Unit>,
        decl: &ast::UseDecl,
        out: &mut Vec<Rc<Unit>>,
        stack: &mut Vec<String>,
    ) -> Result<()> {
        let written = decl.path.display();

        // `std` is the built-in library: the intrinsic modules are always in
        // scope and the prelude is imported automatically, so a `use` of one is
        // accepted and does nothing.
        if let Some(rest) = written.strip_prefix("std::") {
            if crate::stdlib::modules().contains(&rest) || rest == "prelude" {
                return Ok(());
            }
            return Err(Error::new(
                from.source
                    .error(decl.path.span.pos, format!("there is no `std::{rest}`"))
                    .note(format!(
                        "the built-in modules are {} and `prelude`",
                        crate::stdlib::modules().join(", ")
                    )),
            ));
        }

        let (key, item) = self.locate(&decl.path, from)?;
        let unit = self.load_key(&key, from, decl.path.span.pos)?;
        if unit.file.target().is_some() {
            return Err(Error::new(from.source.error(
                decl.path.span.pos,
                format!("`{key}` declares a target and cannot be used as a module"),
            )));
        }
        if let Some(item) = item {
            require_item(&unit, &item, &key, from, decl.path.span.pos)?;
        }
        if let Some(names) = &decl.names {
            for name in names {
                require_item(&unit, name, &key, from, name.span.pos)?;
            }
        }

        if out.iter().any(|u| Rc::ptr_eq(u, &unit)) {
            return Ok(());
        }
        if stack.contains(&unit.key) {
            let mut chain = stack.clone();
            chain.push(unit.key.clone());
            return Err(Error::new(
                from.source
                    .error(decl.path.span.pos, format!("`{written}` imports itself"))
                    .note(format!("the cycle is {}", chain.join(" → "))),
            ));
        }
        stack.push(unit.key.clone());
        let nested = unit.file.uses.clone();
        for nested_decl in &nested {
            self.resolve_uses(&unit, nested_decl, out, stack)?;
        }
        stack.pop();
        out.push(unit);
        Ok(())
    }

    /// The module a `use` path refers to, and the item it names if it names one.
    fn locate(&self, path: &ast::Path, from: &Rc<Unit>) -> Result<(String, Option<ast::Ident>)> {
        if let Some(file) = module_file(&self.root, &path.segments) {
            if file.exists() {
                return Ok((path.display(), None));
            }
        }
        if path.segments.len() >= 2 {
            let (head, tail) = path.segments.split_at(path.segments.len() - 1);
            if let Some(file) = module_file(&self.root, head) {
                if file.exists() {
                    let key = head
                        .iter()
                        .map(|s| s.name.as_str())
                        .collect::<Vec<_>>()
                        .join("::");
                    return Ok((key, Some(tail[0].clone())));
                }
            }
        }
        let mut error = Error::new(
            from.source
                .error(
                    path.span.pos,
                    format!("cannot find the module `{}`", path.display()),
                )
                .span(path.span.len),
        );
        let suggestion = path.segments[..path.segments.len().saturating_sub(1)]
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join("::");
        if !suggestion.is_empty() {
            error = error.note(format!("looked for `src/{suggestion}.rav`"));
        }
        Err(error.note("a module path is a file under `src/`, written with `::`"))
    }

    fn load_key(&mut self, key: &str, from: &Rc<Unit>, pos: Pos) -> Result<Rc<Unit>> {
        if let Some(unit) = self.by_key.get(key) {
            return Ok(unit.clone());
        }
        let segments: Vec<&str> = key.split("::").collect();
        let idents: Vec<ast::Ident> = segments
            .iter()
            .map(|s| ast::Ident::new(*s, crate::diag::Span::default()))
            .collect();
        let Some(file) = module_file(&self.root, &idents) else {
            return Err(Error::new(
                from.source.error(pos, format!("cannot find `{key}`")),
            ));
        };
        let unit = self.file_keyed(&file, key)?;
        self.by_key.insert(key.to_string(), unit.clone());
        Ok(unit)
    }

    fn file(&mut self, path: &Path) -> Result<Rc<Unit>> {
        let key = stem(path);
        let key = key.clone();
        self.file_keyed(path, &key)
    }

    fn file_keyed(&mut self, path: &Path, key: &str) -> Result<Rc<Unit>> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(unit) = self.cache.get(&canonical) {
            return Ok(unit.clone());
        }
        let text = std::fs::read_to_string(path).map_err(|e| {
            Error::new(Diag::error(format!(
                "cannot read `{}`",
                display(path, &self.root)
            )))
            .note(e.to_string())
        })?;
        let source = Rc::new(Source::new(relativise(path, &self.root), text));
        let file = parser::parse(&source)?;
        let unit = Rc::new(Unit {
            key: key.to_string(),
            path: path.to_path_buf(),
            source,
            file,
        });
        self.cache.insert(canonical, unit.clone());
        self.files.push(unit.clone());
        Ok(unit)
    }
}

fn require_item(
    unit: &Unit,
    item: &ast::Ident,
    key: &str,
    from: &Rc<Unit>,
    pos: Pos,
) -> Result<()> {
    let found = unit.file.items.iter().any(|candidate| {
        candidate.is_public() && candidate.name().is_some_and(|n| n.name == item.name)
    });
    if found {
        return Ok(());
    }
    let available: Vec<String> = unit
        .file
        .items
        .iter()
        .filter(|candidate| candidate.is_public())
        .filter_map(|candidate| candidate.name().map(|n| format!("`{}`", n.name)))
        .collect();
    let mut error = Error::new(
        from.source
            .error(pos, format!("`{key}` has no public item `{}`", item.name)),
    );
    if !available.is_empty() {
        error = error.note(format!("it exports {}", available.join(", ")));
    }
    error = error.note("an item other files can use must be written `pub`");
    Err(error)
}

/// The file a module path would live in.
fn module_file(root: &Path, segments: &[ast::Ident]) -> Option<PathBuf> {
    if segments.is_empty() {
        return None;
    }
    let mut file = root.join("src");
    for segment in segments {
        file.push(&segment.name);
    }
    file.set_extension(identity::SOURCE_EXTENSION);
    Some(file)
}

/// The key a file is known by: its file stem, unless a module path overrides it.
fn stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("module")
        .to_string()
}

fn relativise(path: &Path, root: &Path) -> PathBuf {
    path.strip_prefix(root)
        .map_or_else(|_| path.to_path_buf(), Path::to_path_buf)
}

fn display(path: &Path, root: &Path) -> String {
    relativise(path, root)
        .display()
        .to_string()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_manifest_is_a_diagnostic() {
        let error = load(Path::new("no/such/raven.toml")).expect_err("must fail");
        assert!(error.render().contains("cannot read"), "{}", error.render());
    }

    #[test]
    fn module_keys_are_file_stems() {
        assert_eq!(stem(Path::new("src/lib/geometry.rav")), "geometry");
        assert_eq!(stem(Path::new("/a/b/thing.rav")), "thing");
    }

    #[test]
    fn a_module_path_maps_to_a_file_under_src() {
        let root = Path::new("/project");
        let ident = |name: &str| ast::Ident::new(name, crate::diag::Span::default());
        let file = module_file(root, &[ident("lib"), ident("geometry")]).expect("a path");
        assert!(file.ends_with("geometry.rav"));
        assert!(file
            .to_string_lossy()
            .replace('\\', "/")
            .contains("src/lib/geometry.rav"));
    }
}
