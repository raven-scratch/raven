//! The raven-asm compiler: source files in, a Scratch 3 project out.
//!
//! The pipeline is deliberately linear, because raven-asm has no desugaring step:
//!
//! 1. [`Loader`] reads each target file and every module it pulls in with `use`.
//! 2. Symbols (variables, lists, broadcasts, costumes, sounds, procedures) are
//!    collected per target, with sprite-to-stage fallback for globals.
//! 3. [`Emitter`] walks the statements and writes Scratch blocks — one block per
//!    statement — plus a monitor record for every variable and list.
//! 4. Assets are hashed and the caller packs the `.sb3`.

use crate::ast::*;
use crate::manifest::Manifest;
use raven_scratch::assets::{self, Asset};
use raven_scratch::catalog::{
    self, fixed_menu_values, menu, BlockKind, BlockSpec, Body, MenuDomain, Shape, Stability, Wire,
};
use raven_scratch::diag::{Diag, Error, Pos, Result, Source};
use raven_scratch::ids::IdGen;
use raven_scratch::sb3;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Everything a successful build produces.
pub struct BuildOutput {
    pub project: sb3::Project,
    pub assets: Vec<Asset>,
    pub warnings: Vec<Diag>,
    pub output_dir: PathBuf,
    pub output_file: String,
}

/// Knobs for a build. The default matches the historical behaviour.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuildOptions {
    /// Reject blocks that vanilla Scratch cannot run. Without this, such a
    /// block is emitted with a warning; with it, it is a hard error.
    pub strict: bool,
}

/// Compile the project described by `manifest_path`.
pub fn build(manifest_path: &Path) -> Result<BuildOutput> {
    build_with_options(manifest_path, BuildOptions::default())
}

/// Compile the project described by `manifest_path` with explicit options.
pub fn build_with_options(manifest_path: &Path, options: BuildOptions) -> Result<BuildOutput> {
    let root = manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let manifest = Manifest::load(manifest_path)?;
    let mut compiler = Compiler {
        root,
        manifest,
        loader: Loader::default(),
        ids: IdGen::new(),
        asset_keys: HashSet::new(),
        warnings: Vec::new(),
        assets: Vec::new(),
        extended_used: HashSet::new(),
        extended_sites: Vec::new(),
        strict: options.strict,
    };
    compiler.run()
}

struct Compiler {
    root: PathBuf,
    manifest: Manifest,
    loader: Loader,
    ids: IdGen,
    asset_keys: HashSet<String>,
    warnings: Vec<Diag>,
    assets: Vec<Asset>,
    extended_used: HashSet<&'static str>,
    extended_sites: Vec<ExtendedSite>,
    strict: bool,
}

/// One use of a block only extended runtimes provide, kept so `--strict` can
/// point at the offending source line.
struct ExtendedSite {
    opcode: &'static str,
    src: Rc<Source>,
    pos: Pos,
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Loader {
    cache: HashMap<PathBuf, Rc<FileUnit>>,
}

struct FileUnit {
    path: PathBuf,
    key: PathBuf,
    src: Rc<Source>,
    uses: Vec<UseDecl>,
    target: Option<TargetDecl>,
    items: Vec<Item>,
}

#[derive(Clone)]
struct ScopedItem {
    src: Rc<Source>,
    item: Item,
}

impl Loader {
    fn load(&mut self, path: &Path) -> Result<Rc<FileUnit>> {
        let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if let Some(unit) = self.cache.get(&key) {
            return Ok(unit.clone());
        }
        let text = std::fs::read_to_string(path).map_err(|e| {
            Error::new(Diag::error(format!("cannot read `{}`", path.display())).note(e.to_string()))
        })?;
        let src = Rc::new(Source::new(path.to_path_buf(), text));
        let file = crate::parser::parse(&src)?;
        // A target file keeps its body inside the target declaration; a module
        // keeps it at file level. Normalise both into `items`.
        let (target, items) = match file.target {
            Some(decl) => {
                let items = decl.items.clone();
                (Some(decl), items)
            }
            None => (None, file.items),
        };
        let unit = Rc::new(FileUnit {
            path: path.to_path_buf(),
            key,
            src,
            uses: file.uses,
            target,
            items,
        });
        self.cache.insert(unit.key.clone(), unit.clone());
        Ok(unit)
    }
}

/// Resolve a `use` path relative to the file that wrote it.
fn resolve_use(unit: &FileUnit, decl: &UseDecl) -> Result<PathBuf> {
    let base = unit.path.parent().unwrap_or(Path::new("."));
    let candidate = base.join(&decl.path);
    if candidate.is_file() {
        return Ok(candidate);
    }
    let with_ext = candidate.with_extension(crate::identity::SOURCE_EXTENSION);
    if candidate.extension().is_none() && with_ext.is_file() {
        return Ok(with_ext);
    }
    Err(Error::new(
        unit.src
            .error(decl.pos, format!("cannot find module `{}`", decl.path))
            .note(format!("looked for `{}`", candidate.display()))
            .note("module paths are relative to the file that uses them"),
    ))
}

/// Flatten a module and everything it uses into `out`.
fn include_module(
    loader: &mut Loader,
    unit: Rc<FileUnit>,
    stack: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
    out: &mut Vec<ScopedItem>,
) -> Result<()> {
    if stack.contains(&unit.key) {
        let chain = stack
            .iter()
            .map(|p| {
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join(" -> ");
        return Err(Error::new(
            unit.src
                .error(Pos::new(1, 1), "circular module inclusion")
                .note(format!("{chain} -> {}", unit.path.display())),
        ));
    }
    if !seen.insert(unit.key.clone()) {
        return Ok(());
    }
    if let Some(target) = &unit.target {
        return Err(Error::new(
            unit.src
                .error(
                    target.pos,
                    "an included module must not declare a stage or a sprite",
                )
                .note("modules hold `proc` definitions; targets are listed in raven-asm.toml"),
        ));
    }

    stack.push(unit.key.clone());
    for decl in &unit.uses {
        let path = resolve_use(&unit, decl)?;
        let dep = loader.load(&path)?;
        include_module(loader, dep, stack, seen, out)?;
    }
    stack.pop();

    for item in &unit.items {
        // A module may contribute procedures — copied into every target that
        // uses it, because a Scratch custom block belongs to one target — and
        // declarations that have exactly one instance for the whole project.
        // Anything target-owned it cannot provide, because it has no target.
        let allowed = match item {
            Item::Proc(_) | Item::Broadcast(_) => true,
            Item::Var(v) => v.global,
            Item::List(l) => l.global,
            _ => false,
        };
        if !allowed {
            let decl = item.describe();
            let mut err = unit
                .src
                .error(
                    item.pos(),
                    format!("a module cannot declare a {decl}"),
                )
                .note("a module has no target of its own, so it cannot own a variable, list, costume or sound");
            if matches!(item, Item::Var(_) | Item::List(_)) {
                err = err
                    .note("write `global var` / `global list` to declare project-wide state that every user shares, or declare it in the target that needs it");
            } else {
                err = err.note("move it into the target that needs it");
            }
            return Err(Error::new(err));
        }
        out.push(ScopedItem {
            src: unit.src.clone(),
            item: item.clone(),
        });
    }
    Ok(())
}

/// All items belonging to a target: its modules' items first, then its own.
fn target_entries(loader: &mut Loader, unit: Rc<FileUnit>) -> Result<Vec<ScopedItem>> {
    let mut out = Vec::new();
    let mut stack = vec![unit.key.clone()];
    let mut seen: HashSet<PathBuf> = HashSet::new();
    seen.insert(unit.key.clone());
    for decl in &unit.uses {
        let path = resolve_use(&unit, decl)?;
        let dep = loader.load(&path)?;
        include_module(loader, dep, &mut stack, &mut seen, &mut out)?;
    }
    for item in &unit.items {
        out.push(ScopedItem {
            src: unit.src.clone(),
            item: item.clone(),
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Symbols
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Sym {
    name: String,
    id: String,
    /// The value Scratch stores in the target's `variables` / `lists` map.
    init: Value,
    /// Whether the editor's monitor for it starts shown.
    visible: bool,
    /// `at X Y` on the declaration, when it says where to put the monitor.
    monitor: MonitorSpec,
}

#[derive(Clone, Debug)]
struct ProcInfo {
    name: String,
    params: Vec<Param>,
    warp: bool,
    proccode: String,
    arg_ids: Vec<String>,
}

struct TargetSource {
    kind: TargetKind,
    name: String,
    pos: Pos,
    src: Rc<Source>,
    entries: Vec<ScopedItem>,
}

/// Symbols visible while emitting one target.
#[derive(Default)]
struct Syms {
    vars: Vec<Sym>,
    lists: Vec<Sym>,
    costumes: Vec<String>,
    sounds: Vec<String>,
    procs: Vec<ProcInfo>,
}

/// Symbols shared by the whole project.
struct Globals {
    sprite_names: Vec<String>,
    stage_vars: Vec<Sym>,
    stage_lists: Vec<Sym>,
    stage_costumes: Vec<String>,
    /// Broadcast name to id, in declaration order.
    broadcasts: Vec<Sym>,
    broadcast_index: HashMap<String, String>,
}

impl Syms {
    fn var<'a>(&'a self, globals: &'a Globals, name: &str) -> Option<&'a Sym> {
        self.vars
            .iter()
            .find(|s| s.name == name)
            .or_else(|| globals.stage_vars.iter().find(|s| s.name == name))
    }

    fn list<'a>(&'a self, globals: &'a Globals, name: &str) -> Option<&'a Sym> {
        self.lists
            .iter()
            .find(|s| s.name == name)
            .or_else(|| globals.stage_lists.iter().find(|s| s.name == name))
    }

    fn proc(&self, name: &str) -> Option<&ProcInfo> {
        self.procs.iter().find(|p| p.name == name)
    }
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

impl Compiler {
    fn run(&mut self) -> Result<BuildOutput> {
        let stage_path = self.manifest.stage_path(&self.root);
        let sprite_paths = self.manifest.sprite_paths(&self.root);
        let manifest_display = self.root.join(crate::manifest::MANIFEST_NAME);

        // ---- 1. Load and parse every target file.
        let mut targets: Vec<TargetSource> = Vec::new();
        let plan: Vec<(PathBuf, bool)> = std::iter::once((stage_path, true))
            .chain(sprite_paths.into_iter().map(|p| (p, false)))
            .collect();

        for (path, expect_stage) in plan {
            let unit = self.loader.load(&path).map_err(|mut error| {
                // A parse error already names the file and the line. Only a
                // file-level failure needs to say where the path came from.
                if error.diags.first().is_some_and(|d| d.snippet.is_none()) {
                    error = error.note(format!("referenced from `{}`", manifest_display.display()));
                }
                error
            })?;
            let Some(decl) = unit.target.clone() else {
                return Err(Error::new(
                    unit.src
                        .error(Pos::new(1, 1), "this file declares no target")
                        .note("a target file starts with `stage { ... }` or `sprite \"Name\" { ... }`"),
                ));
            };
            if expect_stage && decl.kind != TargetKind::Stage {
                return Err(Error::new(
                    unit.src
                        .error(
                            decl.pos,
                            "`targets.stage` must point at a `stage { ... }` file",
                        )
                        .note("sprite files belong under `targets.sprites`"),
                ));
            }
            if !expect_stage && decl.kind != TargetKind::Sprite {
                return Err(Error::new(unit.src.error(
                    decl.pos,
                    "`targets.sprites` must point at `sprite \"Name\" { ... }` files",
                )));
            }
            let entries = target_entries(&mut self.loader, unit.clone())?;
            targets.push(TargetSource {
                kind: decl.kind,
                name: decl.name,
                pos: decl.pos,
                src: unit.src.clone(),
                entries,
            });
        }

        // ---- 2. Project-wide tables.
        let stage_name = targets
            .iter()
            .find(|t| t.kind == TargetKind::Stage)
            .map(|t| t.name.clone());
        let mut sprite_names: Vec<String> = Vec::new();
        for t in targets.iter().filter(|t| t.kind == TargetKind::Sprite) {
            if stage_name.as_deref() == Some(t.name.as_str()) {
                return Err(Error::new(
                    t.src
                        .error(t.pos, format!("a sprite cannot be named `{}`", t.name))
                        .note("`Stage` is the reserved name of the stage target")
                        .note("rename the sprite"),
                ));
            }
            if sprite_names.contains(&t.name) {
                return Err(Error::new(
                    t.src
                        .error(t.pos, format!("duplicate sprite name `{}`", t.name)),
                ));
            }
            sprite_names.push(t.name.clone());
        }

        // Broadcast messages are global: they live on the stage whichever file
        // declares them.
        let mut broadcast_names: Vec<String> = Vec::new();
        let mut broadcast_origin: HashMap<String, Pos> = HashMap::new();
        for t in &targets {
            for entry in &t.entries {
                if let Item::Broadcast(b) = &entry.item {
                    if !broadcast_names.contains(&b.name) {
                        broadcast_names.push(b.name.clone());
                        broadcast_origin.insert(b.name.clone(), b.pos);
                    }
                }
            }
        }
        let broadcasts: Vec<Sym> = broadcast_names
            .iter()
            .map(|name| Sym {
                name: name.clone(),
                id: self.ids.fresh(&format!("broadcast:{name}")),
                init: Value::Null,
                visible: false,
                monitor: MonitorSpec::default(),
            })
            .collect();
        let broadcast_index: HashMap<String, String> = broadcasts
            .iter()
            .map(|s| (s.name.clone(), s.id.clone()))
            .collect();

        // ---- 3. Project-wide variables and lists.
        //
        // These live on the stage whichever file declares them: the stage's own
        // `var` (global by position, which is the natural reading) and any
        // `global var` anywhere else, including inside a module. Collecting them
        // in one pass means a name is bound to exactly one variable for the whole
        // project, so a module cannot mean two different things to two users.
        let (stage_vars, stage_lists) = self.collect_globals(&targets)?;

        // ---- 4. Per-target symbols. Collected once so identifiers stay stable.
        let mut syms_per_target: Vec<Syms> = Vec::new();
        for target in &targets {
            syms_per_target.push(self.collect_syms(target, &stage_vars, &stage_lists)?);
        }

        let Some(stage_index) = targets.iter().position(|t| t.kind == TargetKind::Stage) else {
            return Err(Error::new(Diag::error(
                "internal error: the project has no stage target",
            )));
        };
        let globals = Globals {
            sprite_names,
            stage_vars,
            stage_lists,
            stage_costumes: syms_per_target[stage_index].costumes.clone(),
            broadcasts,
            broadcast_index,
        };

        // ---- 4. Emit.
        let project_root = self.root.clone();
        let mut sb3_targets: Vec<sb3::Target> = Vec::new();
        let mut monitors: Vec<sb3::Monitor> = Vec::new();
        let mut extensions: HashSet<String> =
            self.manifest.project.extensions.iter().cloned().collect();

        for (index, target) in targets.iter().enumerate() {
            let syms = &syms_per_target[index];
            let is_stage = target.kind == TargetKind::Stage;

            let mut emitter = Emitter {
                name: target.name.clone(),
                root: &project_root,
                syms,
                globals: &globals,
                ids: &mut self.ids,
                asset_keys: &mut self.asset_keys,
                blocks: BTreeMap::new(),
                assets: Vec::new(),
                warnings: Vec::new(),
                costumes: Vec::new(),
                sounds: Vec::new(),
                script_index: 0,
                extensions: HashSet::new(),
                extended_used: HashSet::new(),
                extended_sites: Vec::new(),
                param_stack: Vec::new(),
            };
            emitter.emit_target(&target.entries)?;

            if emitter.costumes.is_empty() {
                return Err(Error::new(
                    target
                        .src
                        .error(
                            target.pos,
                            format!("`{}` declares no costumes", target.name),
                        )
                        .note("every target needs at least one `costume` declaration")
                        .note("add `costume \"costume1\" = \"assets/file.svg\";`"),
                ));
            }

            let mut target_json = if is_stage {
                sb3::Target::new_stage(&target.name)
            } else {
                sb3::Target::new_sprite(&target.name, sb3_targets.len() as i64)
            };
            target_json.blocks = std::mem::take(&mut emitter.blocks);
            tidy_layout(&mut target_json.blocks);
            target_json.costumes = std::mem::take(&mut emitter.costumes);
            target_json.sounds = std::mem::take(&mut emitter.sounds);

            // Project-wide variables live on the stage; a sprite carries only its
            // own. A `global var` written in a sprite file is therefore not in
            // that sprite's own table — it is in the stage's, and the sprite
            // reaches it the same way it reaches any other global.
            let (vars, lists): (&[Sym], &[Sym]) = if is_stage {
                (&globals.stage_vars, &globals.stage_lists)
            } else {
                (&syms.vars, &syms.lists)
            };
            let owner = if is_stage {
                None
            } else {
                Some(target.name.clone())
            };

            // Monitors are laid out down the left edge, one row each, so two of
            // them never sit on top of each other — the editor's arrangement,
            // with half the gap it leaves. An explicit `at X Y` wins.
            let mut monitor_y = MONITOR_TOP;
            for var in vars {
                target_json
                    .variables
                    .insert(var.id.clone(), vec![json!(var.name), var.init.clone()]);
                monitors.push(scalar_monitor(var, owner.clone(), monitor_y));
                monitor_y += MONITOR_STEP;
            }
            for list in lists {
                target_json
                    .lists
                    .insert(list.id.clone(), vec![json!(list.name), list.init.clone()]);
                monitors.push(list_monitor(list, owner.clone(), monitor_y));
                monitor_y += LIST_MONITOR_STEP;
            }

            // Broadcast messages always live on the stage.
            if is_stage {
                for b in &globals.broadcasts {
                    target_json.broadcasts.insert(b.id.clone(), b.name.clone());
                }
            }

            extensions.extend(emitter.extensions.iter().cloned());
            self.extended_used
                .extend(emitter.extended_used.iter().copied());
            self.extended_sites.append(&mut emitter.extended_sites);
            self.warnings.extend(std::mem::take(&mut emitter.warnings));
            self.assets.extend(std::mem::take(&mut emitter.assets));
            sb3_targets.push(target_json);
        }

        // ---- 5. Report anything that needs an extended runtime.
        let mut extended: Vec<&str> = self.extended_used.iter().copied().collect();
        extended.sort_unstable();
        if self.strict && !self.extended_sites.is_empty() {
            let site = &self.extended_sites[0];
            let mut diag = site
                .src
                .error(
                    site.pos,
                    format!(
                        "`{}` is a block vanilla Scratch does not have",
                        site.opcode
                    ),
                )
                .note("`--strict` rejects blocks that only extended Scratch runtimes provide")
                .note("vanilla Scratch silently skips blocks it does not know, so the project would not run as written")
                .note("remove the block, or build without `--strict`");
            if extended.len() > 1 {
                diag = diag.note(format!("this project also uses: {}", extended.join(", ")));
            }
            return Err(Error::new(diag));
        }
        if !extended.is_empty() {
            self.warnings.push(
                Diag::warning(format!(
                    "this project uses {} block(s) that vanilla Scratch does not have",
                    extended.len()
                ))
                .note(extended.join(", "))
                .note("these run on TurboWarp and on other extended Scratch runtimes"),
            );
        }

        let mut extension_list: Vec<String> = extensions.into_iter().collect();
        extension_list.sort();

        let project = sb3::Project {
            targets: sb3_targets,
            monitors,
            extensions: extension_list,
            extension_urls: BTreeMap::new(),
            meta: sb3::Meta {
                semver: sb3::SB3_SEMVER.to_string(),
                vm: sb3::SB3_VM_VERSION.to_string(),
                agent: crate::identity::agent(),
            },
        };

        Ok(BuildOutput {
            project,
            assets: std::mem::take(&mut self.assets),
            warnings: std::mem::take(&mut self.warnings),
            output_dir: self.manifest.output_dir(&self.root),
            output_file: self.manifest.output_file(),
        })
    }

    /// Gather every project-wide variable and list, in source order, rejecting
    /// duplicate names.
    ///
    /// A declaration is project-wide when the stage declares it (positional, the
    /// natural reading) or when it is written `global`, anywhere — a sprite file
    /// or a module. Everything ends up on the stage, because that is the only
    /// target Scratch lets every sprite read and write.
    fn collect_globals(&mut self, targets: &[TargetSource]) -> Result<(Vec<Sym>, Vec<Sym>)> {
        let mut vars: Vec<Sym> = Vec::new();
        let mut lists: Vec<Sym> = Vec::new();
        // A module used by two sprites is inlined into both, so its declarations
        // are seen twice. Identity is the declaration site, not the inclusion.
        let mut seen_decls: HashSet<(PathBuf, u32, u32, String)> = HashSet::new();
        let mut seen_names: HashMap<String, (Pos, Rc<Source>)> = HashMap::new();

        for target in targets {
            let stage_owns_everything = target.kind == TargetKind::Stage;
            for entry in &target.entries {
                let (kind, name, marked_global, pos) = match &entry.item {
                    Item::Var(v) => ("variable", v.name.as_str(), v.global, v.pos),
                    Item::List(l) => ("list", l.name.as_str(), l.global, l.pos),
                    _ => continue,
                };
                if !marked_global && !stage_owns_everything {
                    continue;
                }

                let key = format!("{kind}:{name}");
                let decl_site = (entry.src.path.clone(), pos.line, pos.col, key.clone());
                if !seen_decls.insert(decl_site) {
                    // The same declaration, reached through a second target.
                    continue;
                }

                if let Some((first_pos, first_src)) = seen_names.get(&key) {
                    return Err(Error::new(
                        entry
                            .src
                            .error(pos, format!("duplicate global {kind} `{name}`"))
                            .note(format!(
                                "already declared at {}:{}",
                                first_src.display_path(),
                                first_pos.line
                            ))
                            .note("project-wide names must be unique across every file"),
                    ));
                }
                seen_names.insert(key, (pos, entry.src.clone()));

                match &entry.item {
                    Item::Var(v) => vars.push(Sym {
                        name: v.name.clone(),
                        id: self.ids.fresh(&format!("global:var:{name}")),
                        init: v.init.json(),
                        visible: v.visible,
                        monitor: v.monitor,
                    }),
                    Item::List(l) => lists.push(Sym {
                        name: l.name.clone(),
                        id: self.ids.fresh(&format!("global:list:{name}")),
                        init: Value::Array(l.init.iter().map(Literal::json).collect()),
                        visible: l.visible,
                        monitor: l.monitor,
                    }),
                    _ => unreachable!("filtered above"),
                }
            }
        }
        Ok((vars, lists))
    }

    /// Register every declaration a target *owns*, in source order.
    ///
    /// Project-wide variables and lists are not owned by the target that happens
    /// to mention them; [`Compiler::collect_globals`] has already taken them, so
    /// they are skipped here and reached through the globals table instead.
    fn collect_syms(
        &mut self,
        target: &TargetSource,
        global_vars: &[Sym],
        global_lists: &[Sym],
    ) -> Result<Syms> {
        let mut syms = Syms::default();
        let mut seen_names: HashMap<String, (String, Pos)> = HashMap::new();

        for entry in &target.entries {
            let src = &entry.src;
            let claim = |kind: &str,
                         name: &str,
                         pos: Pos,
                         seen: &mut HashMap<String, (String, Pos)>|
             -> Result<()> {
                let key = format!("{kind}:{name}");
                if let Some((first_kind, first_pos)) = seen.get(&key) {
                    return Err(Error::new(
                        src.error(pos, format!("duplicate {kind} `{name}`"))
                            .note(format!(
                                "first declared as {first_kind} at line {}",
                                first_pos.line
                            )),
                    ));
                }
                seen.insert(key, (kind.to_string(), pos));
                Ok(())
            };

            match &entry.item {
                Item::Var(v) => {
                    if v.global || target.kind == TargetKind::Stage {
                        continue;
                    }
                    // A local with the same name as a global would make every
                    // reference ambiguous to a reader, even though Scratch
                    // permits it.
                    if global_vars.iter().any(|g| g.name == v.name) {
                        return Err(Error::new(
                            src.error(
                                v.pos,
                                format!(
                                    "`{}` is already a global variable, so this one would shadow it",
                                    v.name
                                ),
                            )
                            .note(format!(
                                "the stage declares a variable named `{}`",
                                v.name
                            ))
                            .note("rename one of them, or drop `var` here and use the global"),
                        ));
                    }
                    claim("variable", &v.name, v.pos, &mut seen_names)?;
                    syms.vars.push(Sym {
                        name: v.name.clone(),
                        id: self.ids.fresh(&format!("var:{}:{}", target.name, v.name)),
                        init: v.init.json(),
                        visible: v.visible,
                        monitor: v.monitor,
                    });
                }
                Item::List(l) => {
                    if l.global || target.kind == TargetKind::Stage {
                        continue;
                    }
                    if global_lists.iter().any(|g| g.name == l.name) {
                        return Err(Error::new(
                            src.error(
                                l.pos,
                                format!(
                                    "`{}` is already a global list, so this one would shadow it",
                                    l.name
                                ),
                            )
                            .note(format!("the stage declares a list named `{}`", l.name))
                            .note("rename one of them, or drop `list` here and use the global"),
                        ));
                    }
                    claim("list", &l.name, l.pos, &mut seen_names)?;
                    syms.lists.push(Sym {
                        name: l.name.clone(),
                        id: self.ids.fresh(&format!("list:{}:{}", target.name, l.name)),
                        init: Value::Array(l.init.iter().map(Literal::json).collect()),
                        visible: l.visible,
                        monitor: l.monitor,
                    });
                }
                Item::Costume(c) => {
                    claim("costume", &c.name, c.pos, &mut seen_names)?;
                    syms.costumes.push(c.name.clone());
                }
                Item::Sound(s) => {
                    claim("sound", &s.name, s.pos, &mut seen_names)?;
                    syms.sounds.push(s.name.clone());
                }
                Item::Broadcast(_) => {}
                Item::Proc(p) => {
                    // A call is a bare name, so a procedure that reuses a block
                    // name would silently take over that block for every script
                    // in the target.
                    if let Some(spec) = catalog::block(&p.name) {
                        return Err(Error::new(
                            src.error(
                                p.pos,
                                format!(
                                    "`{}` is a Scratch block, so it cannot also be a procedure",
                                    p.name
                                ),
                            )
                            .note(format!("it already means `{}`", spec.text))
                            .note(
                                "rename the procedure; `raven-asm catalog` lists every taken name",
                            ),
                        ));
                    }
                    claim("procedure", &p.name, p.pos, &mut seen_names)?;
                    let mut proccode = p.name.clone();
                    let mut arg_ids = Vec::new();
                    for param in &p.params {
                        proccode.push_str(" %");
                        proccode.push(param.kind.proccode_letter());
                        arg_ids.push(
                            self.ids
                                .fresh(&format!("arg:{}:{}:{}", target.name, p.name, param.name)),
                        );
                    }
                    syms.procs.push(ProcInfo {
                        name: p.name.clone(),
                        params: p.params.clone(),
                        warp: p.warp,
                        proccode,
                        arg_ids,
                    });
                }
                Item::Stmt(_) => {}
            }
        }
        Ok(syms)
    }
}

/// The `mode` string the editor expects.
fn mode_name(mode: MonitorMode) -> &'static str {
    match mode {
        MonitorMode::Default => "default",
        MonitorMode::Large => "large",
        MonitorMode::Slider => "slider",
    }
}

/// Where a monitor goes when the declaration does not say, and how far apart
/// two of them are.
///
/// The editor's own cascade cannot be reproduced without a DOM — it measures a
/// rendered monitor, which a compiler cannot — so a declaration that says
/// nothing gets the editor's starting corner and a row of its own. A default
/// readout renders about 30 units tall, so 38 leaves half the ~15-unit gap the
/// editor's arrangement would.
const MONITOR_LEFT: f64 = 5.0;
const MONITOR_TOP: f64 = 5.0;
const MONITOR_STEP: f64 = 38.0;
/// A list monitor renders 100 × 200 until it is resized.
const LIST_MONITOR_STEP: f64 = 205.0;

/// A variable's monitor record. `fallback_y` is where it goes when the
/// declaration does not say `at X Y`.
fn scalar_monitor(var: &Sym, sprite_name: Option<String>, fallback_y: f64) -> sb3::Monitor {
    let mut params = BTreeMap::new();
    params.insert("VARIABLE".to_string(), var.name.clone());
    let y = var.monitor.at.map_or(fallback_y, |(_, y)| y);
    sb3::Monitor {
        id: var.id.clone(),
        mode: mode_name(var.monitor.mode).to_string(),
        opcode: "data_variable".to_string(),
        params,
        sprite_name,
        value: var.init.clone(),
        width: 0.0,
        height: 0.0,
        x: Some(var.monitor.at.map_or(MONITOR_LEFT, |(x, _)| x)),
        y: Some(y),
        visible: var.visible,
        slider_min: Some(var.monitor.slider.map_or(0.0, |(min, _)| min)),
        slider_max: Some(var.monitor.slider.map_or(100.0, |(_, max)| max)),
        is_discrete: Some(!var.monitor.continuous),
    }
}

/// A list's monitor record: always a list, and never a slider.
fn list_monitor(list: &Sym, sprite_name: Option<String>, fallback_y: f64) -> sb3::Monitor {
    let mut params = BTreeMap::new();
    params.insert("LIST".to_string(), list.name.clone());
    let y = list.monitor.at.map_or(fallback_y, |(_, y)| y);
    sb3::Monitor {
        id: list.id.clone(),
        mode: "list".to_string(),
        opcode: "data_listcontents".to_string(),
        params,
        sprite_name,
        value: list.init.clone(),
        width: 0.0,
        height: 0.0,
        x: Some(list.monitor.at.map_or(MONITOR_LEFT, |(x, _)| x)),
        y: Some(y),
        visible: list.visible,
        slider_min: None,
        slider_max: None,
        is_discrete: None,
    }
}

// ---------------------------------------------------------------------------
// Emitter
// ---------------------------------------------------------------------------

struct Emitter<'a> {
    name: String,
    /// Project root: the directory holding `raven-asm.toml`. Asset paths resolve
    /// against it, so `assets/x.svg` means the same thing in every file.
    root: &'a Path,
    syms: &'a Syms,
    globals: &'a Globals,
    ids: &'a mut IdGen,
    asset_keys: &'a mut HashSet<String>,
    blocks: BTreeMap<String, Value>,
    assets: Vec<Asset>,
    warnings: Vec<Diag>,
    costumes: Vec<sb3::Costume>,
    sounds: Vec<sb3::Sound>,
    script_index: usize,
    extensions: HashSet<String>,
    extended_used: HashSet<&'static str>,
    extended_sites: Vec<ExtendedSite>,
    /// Parameter lists of the procedures currently being emitted.
    param_stack: Vec<Vec<Param>>,
}

/// Scratch's `primitiveOpcodeInfoMap` type tags.
mod prim {
    pub const NUMBER: u64 = 4;
    pub const POSITIVE: u64 = 5;
    pub const WHOLE: u64 = 6;
    pub const INTEGER: u64 = 7;
    pub const ANGLE: u64 = 8;
    pub const COLOR: u64 = 9;
    pub const TEXT: u64 = 10;
    pub const BROADCAST: u64 = 11;
    pub const VARIABLE: u64 = 12;
    pub const LIST: u64 = 13;
}

impl<'a> Emitter<'a> {
    // ------------------------------------------------------------- target

    fn emit_target(&mut self, entries: &[ScopedItem]) -> Result<()> {
        // Assets first, so a malformed costume is reported before its blocks.
        for entry in entries {
            match &entry.item {
                Item::Costume(c) => {
                    let costume = self.load_costume(c, &entry.src)?;
                    self.costumes.push(costume);
                }
                Item::Sound(s) => {
                    let sound = self.load_sound(s, &entry.src)?;
                    self.sounds.push(sound);
                }
                _ => {}
            }
        }

        for entry in entries {
            match &entry.item {
                Item::Proc(p) => self.emit_proc(p, &entry.src)?,
                Item::Stmt(s) => self.emit_script(s, &entry.src)?,
                _ => {}
            }
        }
        Ok(())
    }

    fn load_costume(&mut self, decl: &CostumeDecl, src: &Rc<Source>) -> Result<sb3::Costume> {
        let path = self.root.join(&decl.path);
        let asset = assets::load(&path, src, decl.path_pos)?;
        if !asset.is_image() {
            return Err(Error::new(
                src.error(decl.path_pos, format!("`{}` is not an image", decl.path))
                    .note("costumes must be .svg, .png, .jpg, .bmp or .gif"),
            ));
        }
        let size = assets::image_size(&asset);
        let center = match (decl.center, size) {
            (Some(c), _) => c,
            (None, Some((w, h))) => (w / 2.0, h / 2.0),
            (None, None) => {
                // Scratch's own default is the middle of the costume, so a
                // centre it cannot work out is a bug in the artwork, not
                // something to guess at: say where the middle is.
                return Err(Error::new(
                    src.error(
                        decl.pos,
                        format!(
                            "cannot work out the size of `{}`, so its rotation centre is unknown",
                            decl.path
                        ),
                    )
                    .note(
                        "give the image a `width` and `height`, or a `viewBox`, in its `<svg>` tag",
                    )
                    .note(format!(
                        "or place it yourself: `costume \"{}\" = \"{}\" center X Y;`",
                        decl.name, decl.path
                    )),
                ));
            }
        };

        let filename = asset.filename();
        if self.asset_keys.insert(filename.clone()) {
            self.assets.push(asset.clone());
        }

        Ok(sb3::Costume {
            asset_id: asset.md5.clone(),
            name: decl.name.clone(),
            // Scratch's own serializer always writes this key, SVG included.
            bitmap_resolution: Some(1),
            md5ext: filename,
            data_format: asset.data_format.clone(),
            rotation_center_x: center.0,
            rotation_center_y: center.1,
        })
    }

    fn load_sound(&mut self, decl: &SoundDecl, src: &Rc<Source>) -> Result<sb3::Sound> {
        let path = self.root.join(&decl.path);
        let asset = assets::load(&path, src, decl.path_pos)?;
        if !asset.is_sound() {
            return Err(Error::new(
                src.error(decl.path_pos, format!("`{}` is not a sound", decl.path))
                    .note("sounds must be .wav or .mp3"),
            ));
        }
        let (rate, sample_count) = if asset.data_format == "wav" {
            match assets::wav_info(&asset.data) {
                Some((r, c)) => (Some(r), Some(c)),
                None => (None, None),
            }
        } else {
            (None, None)
        };

        let filename = asset.filename();
        if self.asset_keys.insert(filename.clone()) {
            self.assets.push(asset.clone());
        }

        Ok(sb3::Sound {
            asset_id: asset.md5.clone(),
            name: decl.name.clone(),
            data_format: asset.data_format.clone(),
            format: String::new(),
            rate,
            sample_count,
            md5ext: filename,
        })
    }

    // ----------------------------------------------------------- scripts

    fn emit_script(&mut self, stmt: &Stmt, src: &Rc<Source>) -> Result<()> {
        let spec = catalog::block(&stmt.opcode);
        match spec {
            Some(spec) if spec.kind == BlockKind::Hat => {}
            Some(spec) if spec.kind.is_value() => {
                return Err(Error::new(
                    src.error(stmt.pos, format!("`{}` is a reporter block", stmt.opcode))
                        .span(stmt.len)
                        .note("reporters produce values; they cannot start a script")
                        .note(format!("Scratch block: {}", spec.text)),
                ))
            }
            Some(spec) => {
                return Err(Error::new(
                    src.error(stmt.pos, format!("`{}` cannot start a script", stmt.opcode))
                        .span(stmt.len)
                        .note("a top-level statement must be a hat block")
                        .note("hat blocks are listed under `raven-asm catalog --category events`"),
                )
                .note_if(
                    spec.category == catalog::Category::Control,
                    "`control_start_as_clone` is the other way to start a script",
                ))
            }
            None => {
                if self.syms.proc(&stmt.opcode).is_some() {
                    return Err(Error::new(
                        src.error(
                            stmt.pos,
                            format!("`{}` is a procedure, so it needs a caller", stmt.opcode),
                        )
                        .note("a procedure body runs from a hat block or another procedure"),
                    ));
                }
                return Err(self.unknown_block(stmt, src));
            }
        }

        let id = self.emit_stmt(stmt, src, None)?;
        self.blocks
            .get_mut(&id)
            .expect("just inserted")
            .as_object_mut()
            .expect("block object")
            .remove("parent");
        if let Some(obj) = self.blocks.get_mut(&id).and_then(Value::as_object_mut) {
            obj.insert("parent".to_string(), Value::Null);
        }
        Ok(())
    }

    /// Emit a list of statements as a stack. Returns the first block's id.
    fn emit_statements(
        &mut self,
        stmts: &[Stmt],
        src: &Rc<Source>,
        parent: Option<&str>,
    ) -> Result<Option<String>> {
        let mut first: Option<String> = None;
        let mut prev: Option<String> = None;
        let mut capped_by: Option<(Pos, String)> = None;

        for stmt in stmts {
            if let Some((pos, name)) = &capped_by {
                return Err(Error::new(
                    src.error(
                        stmt.pos,
                        format!("nothing can follow `{name}` from line {pos}"),
                    )
                    .note(format!("`{name}` is a cap block with no bottom notch")),
                ));
            }
            let parent_id = prev.clone().or_else(|| parent.map(str::to_string));
            let id = self.emit_stmt(stmt, src, parent_id.as_deref())?;
            match &prev {
                Some(p) => self.set_next(p, &id),
                None => first = Some(id.clone()),
            }
            capped_by = if self.stmt_is_cap(stmt) {
                Some((stmt.pos, stmt.opcode.clone()))
            } else {
                None
            };
            prev = Some(id);
        }
        Ok(first)
    }

    fn stmt_is_cap(&self, stmt: &Stmt) -> bool {
        match catalog::block(&stmt.opcode) {
            Some(spec) if spec.opcode == "control_stop" => !self.stop_has_next(stmt),
            Some(spec) => spec.kind == BlockKind::Cap,
            None => false,
        }
    }

    fn stop_has_next(&self, stmt: &Stmt) -> bool {
        match stmt.args.first() {
            Some(Expr::Str(value, _)) => {
                matches!(
                    value.as_str(),
                    "other scripts in sprite" | "other scripts in stage"
                )
            }
            _ => false,
        }
    }

    // -------------------------------------------------------- statements

    fn emit_stmt(&mut self, stmt: &Stmt, src: &Rc<Source>, parent: Option<&str>) -> Result<String> {
        if let Some(proc) = self.syms.proc(&stmt.opcode) {
            let proc = proc.clone();
            return self.emit_proc_call(stmt, &proc, src, parent);
        }

        let spec = catalog::block(&stmt.opcode).ok_or_else(|| self.unknown_block(stmt, src))?;

        if spec.kind.is_value() {
            return Err(Error::new(
                src.error(stmt.pos, format!("`{}` is a reporter block", stmt.opcode))
                    .span(stmt.len)
                    .note("reporters produce values; they cannot stand alone as a statement")
                    .note(format!("Scratch block: {}", spec.text)),
            ));
        }
        if spec.kind == BlockKind::Hat && parent.is_some() {
            return Err(Error::new(
                src.error(stmt.pos, format!("`{}` is a hat block", stmt.opcode))
                    .span(stmt.len)
                    .note("hat blocks start a script, so they cannot sit inside another block"),
            ));
        }

        if stmt.body.is_some() && spec.body == Body::None {
            return Err(Error::new(
                src.error(
                    stmt.pos,
                    format!("`{}` does not take a `{{ ... }}` body", stmt.opcode),
                )
                .span(stmt.len)
                .note(format!("Scratch block: {}", spec.text))
                .note(format!("write `{}...;` instead", self.signature(spec))),
            ));
        }
        if stmt.body.is_none() && matches!(spec.body, Body::Substack | Body::SubstackElse) {
            return Err(Error::new(
                src.error(stmt.pos, format!("`{}` needs a body", stmt.opcode))
                    .span(stmt.len)
                    .note(format!("Scratch block: {}", spec.text))
                    .note(format!("write `{} {{ ... }}`", self.signature(spec))),
            ));
        }
        if stmt.else_body.is_some() && spec.body != Body::SubstackElse {
            return Err(Error::new(
                src.error(stmt.pos, format!("`{}` has no second branch", stmt.opcode))
                    .note("only `control_if_else` accepts `else { ... }`"),
            ));
        }

        if let Some(spec_ext) = spec.category.extension() {
            self.extensions.insert(spec_ext.to_string());
        }
        if spec.stability == Stability::Extended {
            self.extended_used.insert(spec.opcode);
            self.extended_sites.push(ExtendedSite {
                opcode: spec.opcode,
                src: Rc::clone(src),
                pos: stmt.pos,
            });
        }

        if spec.opcode == "control_stop" {
            return self.emit_stop(stmt, spec, src, parent);
        }

        self.check_arity(stmt, spec, src)?;
        let id = self.ids.fresh(&format!("{}:{}", self.name, spec.opcode));
        let (inputs, fields) = self.compile_args(stmt, spec, src, &id)?;

        let body_next = match (&stmt.body, spec.body) {
            (Some(body), Body::Next) => self.emit_statements(body, src, Some(&id))?,
            _ => None,
        };
        let mut inputs = inputs;
        if let (Some(body), Body::Substack | Body::SubstackElse) = (&stmt.body, spec.body) {
            if let Some(first) = self.emit_statements(body, src, Some(&id))? {
                inputs.insert("SUBSTACK".to_string(), json!([2, first]));
            }
        }
        if let (Some(body), Body::SubstackElse) = (&stmt.else_body, spec.body) {
            if let Some(first) = self.emit_statements(body, src, Some(&id))? {
                inputs.insert("SUBSTACK2".to_string(), json!([2, first]));
            }
        }

        let top_level = if parent.is_none() {
            Some(self.next_script_xy())
        } else {
            None
        };
        let block = make_block(
            spec.opcode,
            body_next,
            parent,
            inputs,
            fields,
            false,
            top_level,
            None,
        );
        self.blocks.insert(id.clone(), block);
        Ok(id)
    }

    fn emit_stop(
        &mut self,
        stmt: &Stmt,
        spec: &'static BlockSpec,
        src: &Rc<Source>,
        parent: Option<&str>,
    ) -> Result<String> {
        self.check_arity(stmt, spec, src)?;
        let (inputs, fields) = self.compile_args(stmt, spec, src, "__stop__")?;
        let has_next = self.stop_has_next(stmt);
        if !has_next && stmt.body.is_some() {
            return Err(Error::new(
                src.error(
                    stmt.pos,
                    format!(
                        "`control_stop(\"{}\")` takes no body",
                        match &stmt.args[0] {
                            Expr::Str(s, _) => s.clone(),
                            other => other.describe(),
                        }
                    ),
                )
                .note("only `\"other scripts in sprite\"` and `\"other scripts in stage\"` leave a notch for more blocks"),
            ));
        }

        let id = self.ids.fresh(&format!("{}:control_stop", self.name));
        let body_next = match &stmt.body {
            Some(body) => self.emit_statements(body, src, Some(&id))?,
            None => None,
        };
        let mutation =
            has_next.then(|| json!({ "tagName": "mutation", "children": [], "hasnext": "true" }));
        let top_level = if parent.is_none() {
            Some(self.next_script_xy())
        } else {
            None
        };
        let block = make_block(
            "control_stop",
            body_next,
            parent,
            inputs,
            fields,
            false,
            top_level,
            mutation,
        );
        self.blocks.insert(id.clone(), block);
        Ok(id)
    }

    // ------------------------------------------------------- procedures

    fn emit_proc(&mut self, decl: &ProcDecl, src: &Rc<Source>) -> Result<()> {
        let proc = self
            .syms
            .proc(&decl.name)
            .expect("registered during symbol collection")
            .clone();

        let definition_id = self.ids.fresh(&format!("{}:def:{}", self.name, decl.name));
        let prototype_id = self
            .ids
            .fresh(&format!("{}:proto:{}", self.name, decl.name));

        let mut proto_inputs: Map<String, Value> = Map::new();
        for (param, arg_id) in decl.params.iter().zip(proc.arg_ids.iter()) {
            let reporter_opcode = match param.kind {
                ParamKind::Boolean => "argument_reporter_boolean",
                _ => "argument_reporter_string_number",
            };
            let reporter_id = self
                .ids
                .fresh(&format!("{}:param:{}:{}", self.name, decl.name, param.name));
            let mut fields = Map::new();
            fields.insert("VALUE".to_string(), json!([param.name]));
            let reporter = make_block(
                reporter_opcode,
                None,
                Some(&prototype_id),
                BTreeMap::new(),
                map_to_btree(fields),
                true,
                None,
                None,
            );
            self.blocks.insert(reporter_id.clone(), reporter);
            proto_inputs.insert(arg_id.clone(), json!([1, reporter_id]));
        }

        let warp = proc.warp.to_string();
        let mutation = json!({
            "tagName": "mutation",
            "children": [],
            "proccode": proc.proccode,
            "argumentids": serde_json::to_string(&proc.arg_ids).expect("ids serialize"),
            "argumentnames": serde_json::to_string(
                &decl.params.iter().map(|p| p.name.clone()).collect::<Vec<_>>()
            ).expect("names serialize"),
            // Scratch's `scratch3_procedures.js` reads a boolean parameter's
            // default as the string "false"; string and number parameters get "".
            "argumentdefaults": serde_json::to_string(
                &decl.params.iter().map(|p| {
                    if p.kind == ParamKind::Boolean { "false" } else { "" }
                }).collect::<Vec<_>>()
            ).expect("defaults serialize"),
            "warp": warp,
        });

        let prototype = make_block(
            "procedures_prototype",
            None,
            Some(&definition_id),
            map_to_btree(proto_inputs),
            BTreeMap::new(),
            true,
            None,
            Some(mutation),
        );
        self.blocks.insert(prototype_id.clone(), prototype);

        self.param_stack.push(decl.params.clone());
        let body_first = self.emit_statements(&decl.body, src, Some(&definition_id))?;
        self.param_stack.pop();

        let mut inputs = BTreeMap::new();
        inputs.insert("custom_block".to_string(), json!([1, prototype_id]));
        let definition = make_block(
            catalog::PROCEDURES_DEFINITION,
            body_first,
            None,
            inputs,
            BTreeMap::new(),
            false,
            Some(self.next_script_xy()),
            None,
        );
        self.blocks.insert(definition_id, definition);
        Ok(())
    }

    fn emit_proc_call(
        &mut self,
        stmt: &Stmt,
        proc: &ProcInfo,
        src: &Rc<Source>,
        parent: Option<&str>,
    ) -> Result<String> {
        if stmt.body.is_some() {
            return Err(Error::new(
                src.error(
                    stmt.pos,
                    format!("`{}` is a procedure call and takes no body", stmt.opcode),
                )
                .note("write it as a statement ending in `;`"),
            ));
        }
        if stmt.args.len() != proc.params.len() {
            return Err(self.arity_error(
                stmt,
                src,
                &format!(
                    "{}({})",
                    proc.name,
                    proc.params
                        .iter()
                        .map(|p| p.name.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }

        let id = self.ids.fresh(&format!("{}:call:{}", self.name, proc.name));
        let mut inputs: BTreeMap<String, Value> = BTreeMap::new();
        for ((arg_id, param), expr) in proc
            .arg_ids
            .iter()
            .zip(proc.params.iter())
            .zip(stmt.args.iter())
        {
            let shape = match param.kind {
                ParamKind::Boolean => Shape::Bool,
                ParamKind::Number => Shape::Number,
                ParamKind::String => Shape::Text,
            };
            let value = self.compile_value_input(shape, expr, src, &id)?;
            inputs.insert(arg_id.clone(), value);
        }

        let mutation = json!({
            "tagName": "mutation",
            "children": [],
            "proccode": proc.proccode,
            "argumentids": serde_json::to_string(&proc.arg_ids).expect("ids serialize"),
            "warp": proc.warp.to_string(),
        });
        let top_level = if parent.is_none() {
            Some(self.next_script_xy())
        } else {
            None
        };
        let block = make_block(
            "procedures_call",
            None,
            parent,
            inputs,
            BTreeMap::new(),
            false,
            top_level,
            Some(mutation),
        );
        self.blocks.insert(id.clone(), block);
        Ok(id)
    }

    // ------------------------------------------------------- expressions

    fn emit_reporter(&mut self, call: &CallExpr, src: &Rc<Source>, parent: &str) -> Result<String> {
        if self.syms.proc(&call.opcode).is_some() {
            return Err(Error::new(
                src.error(
                    call.pos,
                    format!(
                        "`{}` is a procedure and cannot produce a value",
                        call.opcode
                    ),
                )
                .note("Scratch custom blocks do not return values"),
            ));
        }
        let spec = catalog::block(&call.opcode)
            .ok_or_else(|| self.unknown_block_at(&call.opcode, call.pos, call.len, src))?;
        if !spec.kind.is_value() {
            return Err(Error::new(
                src.error(
                    call.pos,
                    format!("`{}` does not produce a value", call.opcode),
                )
                .span(call.len)
                .note(format!("Scratch block: {}", spec.text))
                .note("only reporter and boolean blocks can be used as arguments"),
            ));
        }
        if spec.body != Body::None {
            return Err(Error::new(src.error(
                call.pos,
                format!("`{}` cannot be used as a value", call.opcode),
            )));
        }
        let stmt_like = Stmt {
            opcode: call.opcode.clone(),
            args: call.args.clone(),
            body: None,
            else_body: None,
            pos: call.pos,
            len: call.len,
        };
        self.check_arity(&stmt_like, spec, src)?;

        if let Some(ext) = spec.category.extension() {
            self.extensions.insert(ext.to_string());
        }
        if spec.stability == Stability::Extended {
            self.extended_used.insert(spec.opcode);
            self.extended_sites.push(ExtendedSite {
                opcode: spec.opcode,
                src: Rc::clone(src),
                pos: call.pos,
            });
        }

        let id = self.ids.fresh(&format!("{}:{}", self.name, spec.opcode));
        let (inputs, fields) = self.compile_args(&stmt_like, spec, src, &id)?;
        let block = make_block(
            spec.opcode,
            None,
            Some(parent),
            inputs,
            fields,
            false,
            None,
            None,
        );
        self.blocks.insert(id.clone(), block);
        Ok(id)
    }

    /// Compile a value into an input descriptor.
    fn compile_value_input(
        &mut self,
        shape: Shape,
        expr: &Expr,
        src: &Rc<Source>,
        parent_id: &str,
    ) -> Result<Value> {
        if shape == Shape::Bool {
            return match expr {
                Expr::Call(call) => {
                    let spec = catalog::block(&call.opcode);
                    let is_boolean = spec.is_some_and(|s| s.kind == BlockKind::Boolean);
                    if !is_boolean {
                        return Err(Error::new(
                            src.error(
                                call.pos,
                                format!("`{}` is not a boolean block", call.opcode),
                            )
                            .note("this input is hexagonal, so it needs a condition"),
                        ));
                    }
                    let id = self.emit_reporter(call, src, parent_id)?;
                    Ok(json!([2, id]))
                }
                other => Err(Error::new(
                    src.error(
                        other.pos(),
                        format!("expected a condition, found {}", other.describe()),
                    )
                    .note(
                        "use a boolean block such as `operator_lt(...)` or `sensing_mousedown()`",
                    ),
                )),
            };
        }

        match expr {
            Expr::Call(call) => {
                let id = self.emit_reporter(call, src, parent_id)?;
                let shadow = default_shadow(shape);
                Ok(json!([3, id, shadow]))
            }
            literal => {
                let shadow = self.literal_shadow(shape, literal, src)?;
                Ok(json!([1, shadow]))
            }
        }
    }

    fn literal_shadow(&self, shape: Shape, expr: &Expr, src: &Rc<Source>) -> Result<Value> {
        let text = match expr {
            Expr::Number(raw, _) => raw.clone(),
            Expr::Str(s, _) => s.clone(),
            Expr::Bool(b, _) => b.to_string(),
            Expr::Call(_) => unreachable!("handled by the caller"),
        };
        let tag = match shape {
            Shape::Number => prim::NUMBER,
            Shape::Positive => prim::POSITIVE,
            Shape::Whole => prim::WHOLE,
            Shape::Integer => prim::INTEGER,
            Shape::Angle => prim::ANGLE,
            Shape::Text => prim::TEXT,
            Shape::Color => {
                if !is_color(&text) {
                    return Err(Error::new(
                        src.error(expr.pos(), format!("`{text}` is not a colour"))
                            .note("write colours as `\"#rrggbb\"`, for example `\"#ff8800\"`"),
                    ));
                }
                prim::COLOR
            }
            other => {
                return Err(Error::new(src.error(
                    expr.pos(),
                    format!("internal error: no literal shadow for {other:?}"),
                )))
            }
        };
        Ok(json!([tag, text]))
    }

    fn resolve_str<'e>(&self, expr: &'e Expr, src: &Rc<Source>, what: &str) -> Result<&'e str> {
        match expr {
            Expr::Str(s, _) => Ok(s),
            other => Err(Error::new(
                src.error(
                    other.pos(),
                    format!("expected {what} as a string, found {}", other.describe()),
                )
                .note("dropdown values are written as string literals"),
            )),
        }
    }

    fn resolve_variable(&self, expr: &Expr, src: &Rc<Source>) -> Result<Sym> {
        let name = self.resolve_str(expr, src, "a variable name")?;
        if let Some(sym) = self.syms.var(self.globals, name) {
            return Ok(sym.clone());
        }
        let mut candidates: Vec<String> = self
            .syms
            .vars
            .iter()
            .chain(self.globals.stage_vars.iter())
            .map(|s| s.name.clone())
            .collect();
        candidates.sort();
        candidates.dedup();
        let mut err = Error::new(
            src.error(expr.pos(), format!("unknown variable `{name}`"))
                .note(format!("declare it with `var {name} = 0;`")),
        );
        if let Some(hint) = nearest(name, candidates.iter().map(String::as_str)) {
            err = err.note(format!("did you mean `{hint}`?"));
        }
        Err(err)
    }

    fn resolve_list(&self, expr: &Expr, src: &Rc<Source>) -> Result<Sym> {
        let name = self.resolve_str(expr, src, "a list name")?;
        if let Some(sym) = self.syms.list(self.globals, name) {
            return Ok(sym.clone());
        }
        let mut candidates: Vec<String> = self
            .syms
            .lists
            .iter()
            .chain(self.globals.stage_lists.iter())
            .map(|s| s.name.clone())
            .collect();
        candidates.sort();
        candidates.dedup();
        let mut err = Error::new(
            src.error(expr.pos(), format!("unknown list `{name}`"))
                .note(format!("declare it with `list {name} = [];`")),
        );
        if let Some(hint) = nearest(name, candidates.iter().map(String::as_str)) {
            err = err.note(format!("did you mean `{hint}`?"));
        }
        Err(err)
    }

    fn resolve_broadcast(&self, expr: &Expr, src: &Rc<Source>) -> Result<(String, String)> {
        let name = self.resolve_str(expr, src, "a broadcast message name")?;
        match self.globals.broadcast_index.get(name) {
            Some(id) => Ok((name.to_string(), id.clone())),
            None => {
                let mut err = Error::new(
                    src.error(expr.pos(), format!("unknown broadcast message `{name}`"))
                        .note(format!("declare it with `broadcast \"{name}\";`")),
                );
                if let Some(hint) = nearest(
                    name,
                    self.globals.broadcasts.iter().map(|s| s.name.as_str()),
                ) {
                    err = err.note(format!("did you mean `{hint}`?"));
                }
                Err(err)
            }
        }
    }

    /// Resolve `argument_reporter_*("name")` against the enclosing procedure.
    ///
    /// The block the caller is building decides which kind of parameter may be
    /// read: `argument_reporter_boolean` for `: bool`, and
    /// `argument_reporter_string_number` for `: str` and `: num`. Reading a
    /// parameter with the wrong reporter compiles into a block of the wrong
    /// shape, so it is rejected rather than silently emitted.
    fn resolve_param(&self, expr: &Expr, src: &Rc<Source>, opcode: &str) -> Result<String> {
        let name = self.resolve_str(expr, src, "a parameter name")?;
        let wants_boolean = opcode == "argument_reporter_boolean";

        let Some(params) = self.param_stack.last() else {
            return Err(Error::new(
                src.error(
                    expr.pos(),
                    "argument reporters only work inside a procedure",
                )
                .note("`argument_reporter_string_number` and `argument_reporter_boolean` read the parameters of the enclosing `proc`"),
            ));
        };

        let Some(param) = params.iter().find(|p| p.name == name) else {
            return Err(Error::new(
                src.error(
                    expr.pos(),
                    format!("`{name}` is not a parameter of this procedure"),
                )
                .note(format!(
                    "its parameters are: {}",
                    params
                        .iter()
                        .map(|p| format!("{}: {}", p.name, p.kind.spelling()))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            ));
        };

        let is_boolean = param.kind == ParamKind::Boolean;
        if is_boolean != wants_boolean {
            let (right, wrong) = if is_boolean {
                (
                    "argument_reporter_boolean",
                    "argument_reporter_string_number",
                )
            } else {
                (
                    "argument_reporter_string_number",
                    "argument_reporter_boolean",
                )
            };
            return Err(Error::new(
                src.error(
                    expr.pos(),
                    format!(
                        "`{name}` is declared `: {}`, so `{wrong}` cannot read it",
                        param.kind.spelling()
                    ),
                )
                .note(format!("read it with `{right}(\"{name}\")`")),
            ));
        }

        Ok(name.to_string())
    }

    /// Resolve a dropdown value against its domain.
    fn resolve_menu_value(&self, menu_id: &str, expr: &Expr, src: &Rc<Source>) -> Result<String> {
        let raw = self.resolve_str(expr, src, "a dropdown value")?;

        if let Some(values) = fixed_menu_values(menu_id) {
            if let Some(canonical) = values.iter().find(|v| v.eq_ignore_ascii_case(raw)) {
                return Ok((*canonical).to_string());
            }
            return Err(Error::new(
                src.error(
                    expr.pos(),
                    format!("`{raw}` is not a valid value for this dropdown"),
                )
                .note(format!("accepted values: {}", values.join(", "))),
            ));
        }

        let Some(spec) = catalog::MENUS.iter().find(|m| m.id == menu_id) else {
            // Open virtual menus (for example `sensing_of`'s property) accept
            // whatever the project needs.
            return Ok(raw.to_string());
        };

        match spec.domain {
            MenuDomain::Open => Ok(raw.to_string()),
            MenuDomain::Fixed(values) => {
                if let Some(canonical) = values.iter().find(|v| v.eq_ignore_ascii_case(raw)) {
                    return Ok((*canonical).to_string());
                }
                Err(Error::new(
                    src.error(
                        expr.pos(),
                        format!("`{raw}` is not a valid value for this dropdown"),
                    )
                    .note(format!("accepted values: {}", values.join(", "))),
                ))
            }
            MenuDomain::Costumes => {
                self.check_member(raw, &self.syms.costumes, "costume", expr, src)
            }
            MenuDomain::Backdrops => {
                self.check_member(raw, &self.globals.stage_costumes, "backdrop", expr, src)
            }
            MenuDomain::Sounds => self.check_member(raw, &self.syms.sounds, "sound", expr, src),
            MenuDomain::Sprites(extras) => {
                let mut values: Vec<&str> = self
                    .globals
                    .sprite_names
                    .iter()
                    .map(String::as_str)
                    .collect();
                values.extend_from_slice(extras);
                if let Some(canonical) = values.iter().find(|v| **v == raw) {
                    return Ok((*canonical).to_string());
                }
                Err(Error::new(
                    src.error(
                        expr.pos(),
                        format!("`{raw}` does not name a sprite in this project"),
                    )
                    .note(format!("sprites: {}", self.globals.sprite_names.join(", ")))
                    .note(format!("built-in options: {}", extras.join(", "))),
                ))
            }
        }
    }

    fn check_member(
        &self,
        raw: &str,
        values: &[String],
        what: &str,
        expr: &Expr,
        src: &Rc<Source>,
    ) -> Result<String> {
        if values.iter().any(|v| v == raw) {
            return Ok(raw.to_string());
        }
        let mut err = Error::new(
            src.error(
                expr.pos(),
                format!("this target has no {what} called `{raw}`"),
            )
            .note(if values.is_empty() {
                format!("no {what}s are declared for this target")
            } else {
                format!("available: {}", values.join(", "))
            }),
        );
        if let Some(hint) = nearest(raw, values.iter().map(String::as_str)) {
            err = err.note(format!("did you mean `{hint}`?"));
        }
        Err(err)
    }

    // ------------------------------------------------------------- args

    fn check_arity(&self, stmt: &Stmt, spec: &BlockSpec, src: &Rc<Source>) -> Result<()> {
        if stmt.args.len() == spec.args.len() {
            return Ok(());
        }
        Err(self.arity_error(stmt, src, &self.signature(spec)))
    }

    fn arity_error(&self, stmt: &Stmt, src: &Rc<Source>, signature: &str) -> Error {
        let given: Vec<String> = stmt.args.iter().map(Expr::describe).collect();
        let expected = catalog::block(&stmt.opcode).map_or(0, |s| s.args.len());
        Error::new(
            src.error(
                stmt.pos,
                format!(
                    "`{}` takes {} argument{}, but {} {} given",
                    stmt.opcode,
                    expected,
                    if expected == 1 { "" } else { "s" },
                    given.len(),
                    if given.len() == 1 { "was" } else { "were" }
                ),
            )
            .span(stmt.len)
            .note(format!("signature: {signature}"))
            .note(if given.is_empty() {
                "no arguments were given".to_string()
            } else {
                format!("you wrote: {}", given.join(", "))
            }),
        )
    }

    fn signature(&self, spec: &BlockSpec) -> String {
        if spec.args.is_empty() {
            return format!("{};", spec.opcode);
        }
        let args: Vec<String> = spec
            .args
            .iter()
            .map(|a| {
                let name = a.name.to_ascii_lowercase();
                match a.shape {
                    Shape::Menu(_) => format!("\"{name}\""),
                    _ => name,
                }
            })
            .collect();
        format!("{}({});", spec.opcode, args.join(", "))
    }

    fn compile_args(
        &mut self,
        stmt: &Stmt,
        spec: &BlockSpec,
        src: &Rc<Source>,
        block_id: &str,
    ) -> Result<(BTreeMap<String, Value>, BTreeMap<String, Value>)> {
        let mut inputs: BTreeMap<String, Value> = BTreeMap::new();
        let mut fields: BTreeMap<String, Value> = BTreeMap::new();

        for (arg, expr) in spec.args.iter().zip(stmt.args.iter()) {
            match (arg.wire, arg.shape) {
                (Wire::Field, Shape::Variable) => {
                    let sym = self.resolve_variable(expr, src)?;
                    fields.insert(arg.name.to_string(), json!([sym.name, sym.id]));
                }
                (Wire::Field, Shape::List) => {
                    let sym = self.resolve_list(expr, src)?;
                    fields.insert(arg.name.to_string(), json!([sym.name, sym.id]));
                }
                (Wire::Field, Shape::Broadcast) => {
                    let (name, id) = self.resolve_broadcast(expr, src)?;
                    fields.insert(arg.name.to_string(), json!([name, id]));
                }
                (Wire::Field, Shape::ParamName) => {
                    let name = self.resolve_param(expr, src, spec.opcode)?;
                    fields.insert(arg.name.to_string(), json!([name]));
                }
                (Wire::Field, Shape::Menu(menu_id)) => {
                    // A menu stored in `fields` is a dropdown baked into the
                    // block; there is no slot for a reporter to occupy.
                    if matches!(expr, Expr::Call(_)) {
                        return Err(self.menu_reporter_error(menu_id, expr, src));
                    }
                    let value = self.resolve_menu_value(menu_id, expr, src)?;
                    fields.insert(arg.name.to_string(), json!([value]));
                }
                (Wire::Field, other) => {
                    return Err(Error::new(src.error(
                        expr.pos(),
                        format!(
                            "internal error: argument `{}` uses {other:?} as a field",
                            arg.name
                        ),
                    )))
                }

                (Wire::Input, Shape::Bool) => {
                    let value = self.compile_value_input(Shape::Bool, expr, src, block_id)?;
                    inputs.insert(arg.name.to_string(), value);
                }
                (Wire::Input, Shape::Variable) => {
                    let sym = self.resolve_variable(expr, src)?;
                    inputs.insert(
                        arg.name.to_string(),
                        json!([1, [prim::VARIABLE, sym.name, sym.id]]),
                    );
                }
                (Wire::Input, Shape::List) => {
                    let sym = self.resolve_list(expr, src)?;
                    inputs.insert(
                        arg.name.to_string(),
                        json!([1, [prim::LIST, sym.name, sym.id]]),
                    );
                }
                (Wire::Input, Shape::Broadcast) => {
                    let (name, id) = self.resolve_broadcast(expr, src)?;
                    inputs.insert(
                        arg.name.to_string(),
                        json!([1, [prim::BROADCAST, name, id]]),
                    );
                }
                (Wire::Input, Shape::Menu(menu_id)) => {
                    let Some(menu_spec) = menu(menu_id) else {
                        return Err(Error::new(src.error(
                            expr.pos(),
                            format!("internal error: unknown menu `{menu_id}`"),
                        )));
                    };
                    if let Expr::Call(call) = expr {
                        if !menu_spec.accept_reporters {
                            return Err(self.menu_reporter_error(menu_id, expr, src));
                        }
                        // A reporter fills the dropdown. Scratch keeps the
                        // shadow menu block behind it, so the input becomes the
                        // obscured-shadow triple `[3, reporter, shadow]`.
                        let default = self.menu_default(menu_spec);
                        let shadow_id = self.emit_menu_shadow(menu_spec, &default, block_id);
                        let reporter = self.emit_reporter(call, src, block_id)?;
                        inputs.insert(arg.name.to_string(), json!([3, reporter, shadow_id]));
                    } else {
                        let value = self.resolve_menu_value(menu_id, expr, src)?;
                        let shadow_id = self.emit_menu_shadow(menu_spec, &value, block_id);
                        inputs.insert(arg.name.to_string(), json!([1, shadow_id]));
                    }
                }
                (Wire::Input, shape) => {
                    let value = self.compile_value_input(shape, expr, src, block_id)?;
                    inputs.insert(arg.name.to_string(), value);
                }
            }
        }
        Ok((inputs, fields))
    }

    fn emit_menu_shadow(&mut self, spec: &catalog::MenuSpec, value: &str, parent: &str) -> String {
        let id = self
            .ids
            .fresh(&format!("{}:menu:{}", self.name, spec.opcode));
        let mut fields = BTreeMap::new();
        fields.insert(spec.field.to_string(), json!([value]));
        let block = make_block(
            spec.opcode,
            None,
            Some(parent),
            BTreeMap::new(),
            fields,
            true,
            None,
            None,
        );
        self.blocks.insert(id.clone(), block);
        id
    }

    /// The value the shadow menu block keeps behind a reporter. Scratch uses the
    /// reporter's value, but the shadow still needs a valid default so the
    /// project round-trips through the editor.
    fn menu_default(&self, spec: &catalog::MenuSpec) -> String {
        let first = |values: &[String]| values.first().cloned().unwrap_or_default();
        match spec.domain {
            MenuDomain::Fixed(values) => values.first().copied().unwrap_or("").to_string(),
            MenuDomain::Costumes => first(&self.syms.costumes),
            MenuDomain::Backdrops => first(&self.globals.stage_costumes),
            MenuDomain::Sounds => first(&self.syms.sounds),
            MenuDomain::Sprites(extras) => self
                .globals
                .sprite_names
                .first()
                .cloned()
                .or_else(|| extras.first().map(|s| (*s).to_string()))
                .unwrap_or_default(),
            // An open menu takes any literal; the shadow only needs one.
            MenuDomain::Open => "1".to_string(),
        }
    }

    /// A reporter cannot fill this dropdown, which is either a fixed field or a
    /// menu whose `acceptReporters` is false.
    fn menu_reporter_error(&self, menu_id: &str, expr: &Expr, src: &Rc<Source>) -> Error {
        Error::new(
            src.error(
                expr.pos(),
                format!("the `{menu_id}` dropdown does not accept a reporter"),
            )
            .note("this menu is a fixed dropdown, so its value must be a string literal")
            .note("only menus Scratch marks `acceptReporters` can be filled by a reporter"),
        )
    }

    // ------------------------------------------------------------ misc

    fn unknown_block(&self, stmt: &Stmt, src: &Rc<Source>) -> Error {
        self.unknown_block_at(&stmt.opcode, stmt.pos, stmt.len, src)
    }

    fn unknown_block_at(&self, opcode: &str, pos: Pos, len: u32, src: &Rc<Source>) -> Error {
        let mut err = Error::new(
            src.error(pos, format!("unknown block `{opcode}`"))
                .span(len)
                .note("raven-asm block names are the exact Scratch opcodes"),
        );
        if let Some(hint) = catalog::suggest(opcode) {
            err = err.note(format!("did you mean `{hint}`?"));
        }
        if let Some(hint) = nearest(opcode, self.syms.procs.iter().map(|p| p.name.as_str())) {
            err = err.note(format!("this target has a procedure called `{hint}`"));
        }
        err = err.note("run `raven-asm catalog` to list every block");
        err
    }

    fn set_next(&mut self, id: &str, next: &str) {
        if let Some(obj) = self.blocks.get_mut(id).and_then(Value::as_object_mut) {
            obj.insert("next".to_string(), json!(next));
        }
    }

    fn next_script_xy(&mut self) -> (f64, f64) {
        let k = self.script_index;
        self.script_index += 1;
        let col = (k / 10) as f64;
        let row = (k % 10) as f64;
        (60.0 + col * 620.0, 60.0 + row * 420.0)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a Scratch block object with the key order the official serializer uses.
#[allow(clippy::too_many_arguments)]
fn make_block(
    opcode: &str,
    next: Option<String>,
    parent: Option<&str>,
    inputs: BTreeMap<String, Value>,
    fields: BTreeMap<String, Value>,
    shadow: bool,
    top_level: Option<(f64, f64)>,
    mutation: Option<Value>,
) -> Value {
    let mut obj = Map::new();
    obj.insert("opcode".to_string(), json!(opcode));
    obj.insert("next".to_string(), next.map_or(Value::Null, |n| json!(n)));
    obj.insert(
        "parent".to_string(),
        parent.map_or(Value::Null, |p| json!(p)),
    );
    obj.insert("inputs".to_string(), Value::Object(btree_to_map(inputs)));
    obj.insert("fields".to_string(), Value::Object(btree_to_map(fields)));
    obj.insert("shadow".to_string(), json!(shadow));
    match top_level {
        Some((x, y)) => {
            obj.insert("topLevel".to_string(), json!(true));
            obj.insert("x".to_string(), json!(x));
            obj.insert("y".to_string(), json!(y));
        }
        None => {
            obj.insert("topLevel".to_string(), json!(false));
        }
    }
    if let Some(m) = mutation {
        obj.insert("mutation".to_string(), m);
    }
    Value::Object(obj)
}

fn btree_to_map(input: BTreeMap<String, Value>) -> Map<String, Value> {
    input.into_iter().collect()
}

/// Arrange the top-level scripts the way the editor's own "Clean up Blocks"
/// does: one column at x = 0, each script below the last, `MIN_BLOCK_HEIGHT`
/// (48 = 12 × GRID_UNIT, the renderer's constant) of air between the bottom of
/// one stack and the top of the next, in the order the scripts were emitted.
///
/// The editor measures a stack by rendering it; a compiler cannot, so a row is
/// counted at 40 units. That is what a row renders as, and the gap the editor
/// uses is larger than the error either way, so no two scripts can overlap even
/// when a row is taller than the estimate.
fn tidy_layout(blocks: &mut BTreeMap<String, Value>) {
    const ROW: f64 = 40.0;
    const SCRIPT_GAP: f64 = 48.0;

    let mut tops: Vec<(f64, f64, String)> = blocks
        .iter()
        .filter(|(_, block)| block["topLevel"] == Value::Bool(true))
        .filter_map(|(id, block)| Some((block["y"].as_f64()?, block["x"].as_f64()?, id.clone())))
        .collect();
    // The order the emitter chose is the order the source has.
    tops.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));

    let mut cursor = 0.0;
    for (_, _, id) in tops {
        let mut rows = 0.0;
        let mut at = Some(id.clone());
        while let Some(current) = at {
            let Some(block) = blocks.get(&current) else {
                break;
            };
            rows += 1.0;
            at = block["next"].as_str().map(str::to_string);
        }
        if let Some(block) = blocks.get_mut(&id).and_then(Value::as_object_mut) {
            block.insert("x".to_string(), json!(0.0));
            block.insert("y".to_string(), json!(cursor));
        }
        cursor += rows * ROW + SCRIPT_GAP;
    }
}

fn map_to_btree(input: Map<String, Value>) -> BTreeMap<String, Value> {
    input.into_iter().collect()
}

/// The shadow Scratch uses behind a reporter that was dropped into an input.
fn default_shadow(shape: Shape) -> Value {
    match shape {
        Shape::Number => json!([prim::NUMBER, ""]),
        Shape::Positive => json!([prim::POSITIVE, ""]),
        Shape::Whole => json!([prim::WHOLE, ""]),
        Shape::Integer => json!([prim::INTEGER, ""]),
        Shape::Angle => json!([prim::ANGLE, ""]),
        Shape::Text => json!([prim::TEXT, ""]),
        Shape::Color => json!([prim::COLOR, "#000000"]),
        _ => json!([prim::TEXT, ""]),
    }
}

fn is_color(text: &str) -> bool {
    let Some(hex) = text.strip_prefix('#') else {
        return false;
    };
    hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())
}

/// The closest candidate within a small edit distance, for "did you mean" hints.
fn nearest<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    for candidate in candidates {
        let d = levenshtein(name, candidate);
        let limit = (name.chars().count() / 3).max(1) + 1;
        if d <= limit && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, candidate));
        }
    }
    best.map(|(_, c)| c.to_string())
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}
