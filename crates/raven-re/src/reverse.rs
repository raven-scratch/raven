//! Reversing a vanilla Scratch 3 project into raven-asm source.
//!
//! raven-asm's whole rule is that one statement is one Scratch block, and this
//! module walks the project in the other direction with the same rule: one block
//! becomes one statement, one custom block becomes one `proc`, one variable
//! declaration becomes one `var`. Nothing is grouped, inlined or renamed except
//! where raven-asm has no syntax for what the project says, and every such case
//! is either reported or encoded deterministically.
//!
//! What raven-asm cannot say is not invented:
//!
//! * a project that is not vanilla Scratch 3 is refused before anything is
//!   written, because a TurboWarp block has no raven-asm spelling at all;
//! * a name that is not an identifier is replaced by [`crate::names::encode`];
//! * Scratch comments, costume `bitmapResolution` and monitors that watch a
//!   reporter rather than a variable have no raven-asm syntax, so they are
//!   reported as warnings and dropped.

use crate::error::{Error, Result, Warning};
use crate::names::{self, Names};
use raven_asm::ast::ParamKind;
use raven_asm::source::{number, quote};
use raven_scratch::catalog::{self, BlockKind, BlockSpec, Body, Shape, Stability, Wire};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Said of everything raven-asm has no syntax for: the reversal still happens,
/// but the rebuilt project is not the same here.
const CANNOT_WRITE: &str = "raven-asm has no syntax for it";

/// Everything the reversal says about the project it refused.
const NOT_SUPPORTED: &str = "raven reverses vanilla Scratch 3 projects only: TurboWarp and any other edit of Scratch or TurboWarp are not supported";

/// The files of a reversed project.
pub struct Decompiled {
    pub name: String,
    pub files: Vec<(PathBuf, Vec<u8>)>,
    pub warnings: Vec<Warning>,
    pub targets: usize,
    pub blocks: usize,
    pub assets: usize,
    pub monitors: usize,
}

impl Decompiled {
    /// Write the project, creating every directory it needs.
    pub fn write(&self, root: &Path) -> Result<()> {
        for (relative, data) in &self.files {
            let path = root.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    Error::msg(format!("cannot create `{}`: {e}", parent.display()))
                })?;
            }
            std::fs::write(&path, data)
                .map_err(|e| Error::msg(format!("cannot write `{}`: {e}", path.display())))?;
        }
        Ok(())
    }
}

/// Reverse one `.sb3` into the files of a raven-asm project.
pub fn decompile(sb3: &[u8], name: &str) -> Result<Decompiled> {
    let archive = crate::zipr::read(sb3)?;
    let json = &archive
        .get("project.json")
        .ok_or_else(|| missing_project(&archive))?
        .data;
    let raw: Value = serde_json::from_slice(json)
        .map_err(|e| Error::msg(format!("`project.json` is not valid JSON: {e}")))?;

    gate(&raw)?;

    let project: ReadProject = serde_json::from_value(raw.clone()).map_err(|e| {
        Error::msg(format!("`project.json` is not a Scratch 3 project: {e}"))
            .note("the file is missing a field the Scratch editor always writes")
    })?;
    if project
        .targets
        .iter()
        .filter(|target| target.is_stage)
        .count()
        > 1
    {
        return Err(Error::msg("the project has more than one stage"));
    }
    let stage_index = project
        .targets
        .iter()
        .position(|target| target.is_stage)
        .ok_or_else(|| {
            Error::msg("the project has no stage").note("raven-asm compiles exactly one stage")
        })?;

    let monitors = monitor_table(&raw);
    let mut warnings: Vec<Warning> = Vec::new();
    let mut files: Vec<(PathBuf, Vec<u8>)> = Vec::new();

    // Project-wide names are taken first, so a sprite's own declaration can be
    // moved aside when the compiler would otherwise see it as a shadow.
    let stage_target = &project.targets[stage_index];
    let mut global_names = Names::with_keywords();
    let (global_vars, _) = read_syms(&stage_target.variables, &mut global_names)?;
    let (global_lists, _) = read_syms(&stage_target.lists, &mut global_names)?;
    let global_vars_by_id = index_by_id(&global_vars);
    let global_vars_by_name = index_by_name(&global_vars);
    let global_lists_by_id = index_by_id(&global_lists);
    let global_lists_by_name = index_by_name(&global_lists);
    let reserved: HashSet<String> = global_vars
        .iter()
        .chain(global_lists.iter())
        .map(|sym| sym.rasm.clone())
        .collect();

    // Broadcast messages are project-wide whichever file declares them.
    let mut declared_broadcasts: BTreeSet<String> = BTreeSet::new();
    for target in &project.targets {
        declared_broadcasts.extend(target.broadcasts.values().cloned());
    }

    let mut plans: Vec<TargetPlan> = Vec::new();
    let mut bodies: Vec<Rendered> = Vec::new();
    let mut sprites: Vec<String> = Vec::new();
    let mut slugs: HashSet<String> = HashSet::new();
    let mut written_assets: HashSet<String> = HashSet::new();
    let mut blocks = 0usize;
    let mut monitors_used = 0usize;
    let mut assets = 0usize;

    for (index, target) in project.targets.iter().enumerate() {
        let is_stage = index == stage_index;
        let (vars, lists) = if is_stage {
            (global_vars.clone(), global_lists.clone())
        } else {
            let mut local = Names::with_keywords();
            local.reserve_all(reserved.iter().map(String::as_str));
            (
                read_syms(&target.variables, &mut local)?.0,
                read_syms(&target.lists, &mut local)?.0,
            )
        };
        if target.costumes.is_empty() {
            return Err(
                Error::msg(format!("the target `{}` has no costume", target.name))
                    .note("every raven-asm target needs at least one costume declaration"),
            );
        }

        // Assets first, so a project missing one is reported before its blocks.
        for costume in &target.costumes {
            let entry = require_asset(&archive, &costume.md5ext, &costume.name, "costume")?;
            if written_assets.insert(costume.md5ext.clone()) {
                files.push((
                    PathBuf::from(crate::identity::ASSET_DIR).join(&costume.md5ext),
                    entry.data.clone(),
                ));
            }
            assets += 1;
        }
        for sound in &target.sounds {
            let entry = require_asset(&archive, &sound.md5ext, &sound.name, "sound")?;
            if written_assets.insert(sound.md5ext.clone()) {
                files.push((
                    PathBuf::from(crate::identity::ASSET_DIR).join(&sound.md5ext),
                    entry.data.clone(),
                ));
            }
            assets += 1;
        }

        let mut proc_names = Names::with_keywords();
        proc_names.reserve_all(catalog::BLOCKS.iter().map(|block| block.opcode));
        proc_names.reserve_all(catalog::MENUS.iter().map(|menu| menu.opcode));
        let procs = collect_procs(&target.blocks, &mut proc_names)?;

        // The target's own declarations win over the project-wide ones, and the
        // project's ids decide which is which.
        let mut var_by_id = global_vars_by_id.clone();
        let mut var_by_name = global_vars_by_name.clone();
        let mut list_by_id = global_lists_by_id.clone();
        let mut list_by_name = global_lists_by_name.clone();
        for sym in &vars {
            var_by_id.insert(sym.id.clone(), sym.rasm.clone());
            var_by_name.insert(sym.original.clone(), sym.rasm.clone());
        }
        for sym in &lists {
            list_by_id.insert(sym.id.clone(), sym.rasm.clone());
            list_by_name.insert(sym.original.clone(), sym.rasm.clone());
        }

        let plan = TargetPlan {
            is_stage,
            name: target.name.clone(),
            vars,
            lists,
            costumes: target.costumes.clone(),
            sounds: target.sounds.clone(),
            blocks: target.blocks.clone(),
            var_by_id,
            var_by_name,
            list_by_id,
            list_by_name,
            procs,
        };
        let rendered = render_target(&plan, &monitors)?;
        blocks += rendered.blocks;
        monitors_used += rendered.monitors;
        warnings.extend(rendered.warnings.iter().cloned());
        bodies.push(rendered);
        plans.push(plan);
    }

    // Every target has been read, so the broadcast set is complete.
    let mut broadcasts: BTreeSet<String> = declared_broadcasts;
    for rendered in &bodies {
        for message in &rendered.broadcasts {
            if !broadcasts.contains(message) {
                warnings.push(
                    Warning::new(format!(
                        "the project sends the broadcast `{message}` without declaring it"
                    ))
                    .note(
                        "raven-asm declares every message it uses, so a `broadcast` line was added",
                    ),
                );
            }
        }
        broadcasts.extend(rendered.broadcasts.iter().cloned());
    }

    // Assemble: the manifest, the stage, one file per sprite, then the assets
    // that were collected along the way.
    let mut project_files: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for (plan, body) in plans.iter().zip(&bodies) {
        let mut text = header(plan);
        text.push_str(if plan.is_stage {
            "stage {\n"
        } else {
            "sprite "
        });
        if !plan.is_stage {
            text.push_str(&quote(&plan.name));
            text.push_str(" {\n");
        }
        if plan.is_stage {
            for message in &broadcasts {
                text.push_str(&format!("    broadcast {};\n", quote(message)));
            }
        }
        text.push_str(&body.declarations);
        text.push_str(&body.body);
        text.push_str("}\n");
        let path = if plan.is_stage {
            PathBuf::from(crate::identity::STAGE_FILE)
        } else {
            let slug = unique_slug(&plan.name, &mut slugs);
            let path = format!(
                "{}/{}.{}",
                crate::identity::SPRITE_DIR,
                slug,
                crate::identity::SOURCE_EXTENSION
            );
            sprites.push(path.clone());
            PathBuf::from(path)
        };
        project_files.push((path, text.into_bytes()));
    }
    // The manifest is written first, and it lists the sprites the loop above
    // just named.
    project_files.insert(
        0,
        (
            PathBuf::from(crate::identity::MANIFEST),
            manifest_text(name, &sprites, &project.extensions).into_bytes(),
        ),
    );
    project_files.extend(files);

    let comments: usize = project
        .targets
        .iter()
        .map(|target| target.comments.len())
        .sum();
    if comments > 0 {
        warnings.push(
            Warning::new(format!("{comments} Scratch comment(s) were dropped"))
                .note(CANNOT_WRITE)
                .note("raven-asm has no comment blocks"),
        );
    }
    let bitmap: usize = project
        .targets
        .iter()
        .flat_map(|target| target.costumes.iter())
        .filter(|costume| costume.bitmap_resolution.is_some_and(|value| value != 1))
        .count();
    if bitmap > 0 {
        warnings.push(
            Warning::new(format!(
                "{bitmap} costume(s) are drawn at a bitmap resolution other than 1"
            ))
            .note(CANNOT_WRITE)
            .note("raven-asm always writes `bitmapResolution` 1"),
        );
    }
    let all_monitors = raw
        .get("monitors")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let dropped = all_monitors.saturating_sub(monitors_used);
    if dropped > 0 {
        warnings.push(
            Warning::new(format!("{dropped} monitor(s) were dropped"))
                .note("raven-asm writes a monitor for a variable or a list and for nothing else")
                .note("the editor also monitors reporters such as `x position`"),
        );
    }

    Ok(Decompiled {
        name: name.to_string(),
        files: project_files,
        warnings,
        targets: project.targets.len(),
        blocks,
        assets,
        monitors: monitors_used,
    })
}

// ---------------------------------------------------------------------------
// The vanilla gate
// ---------------------------------------------------------------------------

/// Refuse anything that is not a vanilla Scratch 3 project.
///
/// The check is on what the project contains, not on who saved it: a TurboWarp
/// file whose blocks are all vanilla still reverses, and a block vanilla
/// Scratch does not have never does.
fn gate(raw: &Value) -> Result<()> {
    if let Some(agent) = raw.pointer("/meta/agent").and_then(Value::as_str) {
        if agent.to_ascii_lowercase().contains("turbowarp") {
            return Err(Error::msg("this project was saved by TurboWarp")
                .note(format!("`meta.agent` is {agent:?}"))
                .note(NOT_SUPPORTED));
        }
    }
    if let Some(urls) = raw.get("extensionURLs").and_then(Value::as_object) {
        if let Some((id, _)) = urls.iter().next() {
            return Err(Error::msg(format!(
                "this project loads the extension `{id}` from a URL"
            ))
            .note("a loaded extension is not part of vanilla Scratch 3")
            .note(NOT_SUPPORTED));
        }
    }
    if let Some(extensions) = raw.get("extensions").and_then(Value::as_array) {
        for extension in extensions {
            let id = extension.as_str().unwrap_or_default();
            if !matches!(id, "pen" | "music") {
                return Err(
                    Error::msg(format!("this project uses the extension `{id}`")).note(
                        "raven-asm bundles `pen` and `music`; every other extension is an edit of Scratch",
                    )
                    .note(NOT_SUPPORTED),
                );
            }
        }
    }

    for target in raw
        .get("targets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(blocks) = target.get("blocks").and_then(Value::as_object) else {
            continue;
        };
        for (id, block) in blocks {
            let Some(block) = block.as_object() else {
                continue;
            };
            let Some(opcode) = block.get("opcode").and_then(Value::as_str) else {
                return Err(Error::msg(format!("the block `{id}` has no opcode")).note(
                    "one raven-asm statement is one Scratch block, so every block needs one",
                ));
            };
            let shadow = block
                .get("shadow")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if shadow {
                // The three things Scratch keeps as shadow blocks are a
                // dropdown menu, a custom block's prototype, and the parameter
                // reporters inside that prototype.
                let menu = catalog::MENUS.iter().any(|menu| menu.opcode == opcode);
                let parameter = catalog::block(opcode).is_some_and(|spec| {
                    spec.args.len() == 1 && matches!(spec.args[0].shape, Shape::ParamName)
                });
                if !menu && !parameter && opcode != "procedures_prototype" {
                    return Err(Error::msg(format!(
                        "`{opcode}` is a shadow block raven-asm does not write"
                    ))
                    .note("only a Scratch dropdown, a custom block's prototype and its parameter reporters are shadow blocks")
                    .note(NOT_SUPPORTED));
                }
                continue;
            }
            match catalog::block(opcode) {
                Some(spec) if spec.stability == Stability::Extended => {
                    return Err(
                        Error::msg(format!("`{opcode}` is not a vanilla Scratch 3 block"))
                            .note("only TurboWarp and other edited Scratch runtimes provide it")
                            .note(NOT_SUPPORTED),
                    );
                }
                Some(_) => {}
                None if opcode == "procedures_definition" || opcode == "procedures_call" => {}
                None => return Err(unknown(opcode)),
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Reading project.json
// ---------------------------------------------------------------------------

/// The parts of `project.json` a reversal reads, with every field optional.
///
/// `raven_scratch::sb3` models what raven-asm *writes*, field for field. What
/// the Scratch editor writes is older and looser than that — `extensions` can be
/// absent, a costume can lack a rotation centre — so this reader is deliberately
/// more forgiving than the writer.
#[derive(Deserialize)]
struct ReadProject {
    #[serde(default)]
    targets: Vec<ReadTarget>,
    #[serde(default)]
    extensions: Vec<String>,
}

#[derive(Deserialize)]
struct ReadTarget {
    #[serde(rename = "isStage", default)]
    is_stage: bool,
    #[serde(default)]
    name: String,
    #[serde(default)]
    variables: BTreeMap<String, Vec<Value>>,
    #[serde(default)]
    lists: BTreeMap<String, Vec<Value>>,
    #[serde(default)]
    broadcasts: BTreeMap<String, String>,
    #[serde(default)]
    blocks: BTreeMap<String, Value>,
    #[serde(default)]
    costumes: Vec<ReadAsset>,
    #[serde(default)]
    sounds: Vec<ReadAsset>,
    #[serde(default)]
    comments: BTreeMap<String, Value>,
}

#[derive(Deserialize, Clone)]
struct ReadAsset {
    #[serde(default)]
    name: String,
    #[serde(rename = "bitmapResolution", default)]
    bitmap_resolution: Option<u32>,
    #[serde(default)]
    md5ext: String,
    #[serde(rename = "rotationCenterX", default)]
    rotation_x: Option<f64>,
    #[serde(rename = "rotationCenterY", default)]
    rotation_y: Option<f64>,
}

/// The editor's monitor record for one variable or list.
#[derive(Clone, Debug, Default)]
struct MonitorInfo {
    mode: String,
    visible: bool,
    x: Option<f64>,
    y: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
    discrete: Option<bool>,
}

/// Every monitor, under the id it watches and under the name it shows.
///
/// Scratch writes a variable's monitor id as the variable's own id, so the id is
/// the index; the name is the fallback for a project whose monitor ids have been
/// rewritten by an editor.
fn monitor_table(raw: &Value) -> HashMap<String, MonitorInfo> {
    let mut out = HashMap::new();
    for monitor in raw
        .get("monitors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let opcode = monitor.get("opcode").and_then(Value::as_str).unwrap_or("");
        if opcode != "data_variable" && opcode != "data_listcontents" {
            continue;
        }
        let number = |key: &str| monitor.get(key).and_then(Value::as_f64);
        let info = MonitorInfo {
            // The editor has written the slider bounds under both spellings.
            mode: monitor
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("default")
                .to_string(),
            visible: monitor
                .get("visible")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            x: number("x"),
            y: number("y"),
            min: number("sliderMin").or_else(|| number("min")),
            max: number("sliderMax").or_else(|| number("max")),
            discrete: monitor.get("isDiscrete").and_then(Value::as_bool),
        };
        if let Some(id) = monitor.get("id").and_then(Value::as_str) {
            out.insert(id.to_string(), info.clone());
        }
        let key = if opcode == "data_variable" {
            "VARIABLE"
        } else {
            "LIST"
        };
        let name = monitor
            .get("params")
            .and_then(|params| params.get(key))
            .and_then(Value::as_str);
        let sprite = monitor
            .get("spriteName")
            .and_then(Value::as_str)
            .unwrap_or("");
        if let Some(name) = name {
            out.insert(name_key(sprite, name), info);
        }
    }
    out
}

fn name_key(sprite: &str, name: &str) -> String {
    format!("{sprite}\u{0}{name}")
}

/// One declared variable or list.
#[derive(Clone, Debug)]
struct Sym {
    id: String,
    original: String,
    rasm: String,
    init: Value,
}

/// Read a `variables` or `lists` table into symbols with raven-asm names.
fn read_syms(table: &BTreeMap<String, Vec<Value>>, names: &mut Names) -> Result<(Vec<Sym>, bool)> {
    let mut out = Vec::new();
    let mut renamed = false;
    for (id, entry) in table {
        let original = entry
            .first()
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let init = entry.get(1).cloned().unwrap_or(Value::Null);
        let rasm = names.claim(&original, id);
        renamed |= rasm != original;
        out.push(Sym {
            id: id.clone(),
            original,
            rasm,
            init,
        });
    }
    // A readable order that does not depend on the ids Scratch happened to pick.
    out.sort_by(|a, b| a.original.cmp(&b.original).then_with(|| a.id.cmp(&b.id)));
    Ok((out, renamed))
}

fn index_by_id(syms: &[Sym]) -> HashMap<String, String> {
    syms.iter()
        .map(|sym| (sym.id.clone(), sym.rasm.clone()))
        .collect()
}

fn index_by_name(syms: &[Sym]) -> HashMap<String, String> {
    syms.iter()
        .map(|sym| (sym.original.clone(), sym.rasm.clone()))
        .collect()
}

// ---------------------------------------------------------------------------
// Custom blocks
// ---------------------------------------------------------------------------

/// Everything a `proc` declaration and its calls need.
#[derive(Clone, Debug)]
struct ProcPlan {
    /// The name written in raven-asm.
    name: String,
    /// Scratch's own code for the block, which is how a call finds it.
    proccode: String,
    /// The prototype's argument ids, in the order a call passes them.
    arg_ids: Vec<String>,
    /// `(original parameter name, raven-asm name, kind)`, in the same order.
    params: Vec<(String, String, ParamKind)>,
    warp: bool,
}

/// Collect every custom block of one target, keyed by proccode.
///
/// This runs before anything is written because a call can appear above the
/// definition in the file, and because the compiler resolves procedures by name
/// across the whole target.
fn collect_procs(
    blocks: &BTreeMap<String, Value>,
    names: &mut Names,
) -> Result<HashMap<String, ProcPlan>> {
    let mut out = HashMap::new();
    for block in blocks.values() {
        if block.get("opcode").and_then(Value::as_str) != Some("procedures_definition") {
            continue;
        }
        let plan = read_proc(block, blocks, names)?;
        if out.insert(plan.proccode.clone(), plan.clone()).is_some() {
            return Err(Error::msg(format!(
                "two custom blocks are both called `{}`",
                plan.proccode
            ))
            .note("a Scratch custom block is identified by its code, which carries its name and its parameters"));
        }
    }
    Ok(out)
}

fn read_proc(
    definition: &Value,
    blocks: &BTreeMap<String, Value>,
    names: &mut Names,
) -> Result<ProcPlan> {
    let prototype_id = definition
        .get("inputs")
        .and_then(|inputs| inputs.get("custom_block"))
        .and_then(Value::as_array)
        .and_then(|input| input.get(1))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            Error::msg("a custom block has no prototype")
                .note("the prototype is where Scratch keeps the block's name and parameters")
        })?;
    let prototype = blocks.get(prototype_id).ok_or_else(|| {
        Error::msg(format!(
            "a custom block refers to the prototype `{prototype_id}`, which the project does not have"
        ))
    })?;
    let mutation = prototype.get("mutation").ok_or_else(|| {
        Error::msg("a custom block's prototype has no mutation")
            .note("the mutation holds the block's proccode and argument ids")
    })?;
    let proccode = mutation
        .get("proccode")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::msg("a custom block's prototype has no proccode"))?;

    let arg_ids = string_list(mutation.get("argumentids")).unwrap_or_default();
    let argument_names = string_list(mutation.get("argumentnames")).unwrap_or_default();
    let (label, kinds) = proccode_parts(proccode, arg_ids.len()).ok_or_else(|| {
        Error::msg(format!(
            "the custom block `{proccode}` does not follow Scratch's proccode form"
        ))
        .note("a proccode ends in one ` %s`, ` %n` or ` %b` per parameter, and raven-asm writes exactly that")
    })?;

    let mut params = Vec::new();
    let mut param_names = Names::with_keywords();
    for (index, arg_id) in arg_ids.iter().enumerate() {
        let original = prototype
            .get("inputs")
            .and_then(|inputs| inputs.get(arg_id))
            .and_then(Value::as_array)
            .and_then(|input| input.get(1))
            .and_then(Value::as_str)
            .and_then(|id| blocks.get(id))
            .and_then(|reporter| reporter.get("fields"))
            .and_then(|fields| fields.get("VALUE"))
            .and_then(|value| value.as_array())
            .and_then(|value| value.first())
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| argument_names.get(index).cloned())
            .ok_or_else(|| {
                Error::msg(format!(
                    "the custom block `{proccode}` does not name its parameter {}",
                    index + 1
                ))
            })?;
        let kind = kinds.get(index).copied().unwrap_or(ParamKind::String);
        let rasm = param_names.claim(&original, arg_id);
        params.push((original, rasm, kind));
    }

    Ok(ProcPlan {
        name: names.claim(&label, proccode),
        proccode: proccode.to_string(),
        arg_ids,
        params,
        warp: mutation.get("warp").and_then(Value::as_str) == Some("true"),
    })
}

/// Split a proccode into its label and one kind per parameter.
///
/// A proccode is the block's text with `%s`, `%n` or `%b` wherever a parameter
/// goes, and a project may put a parameter anywhere — `找字 %s 大写: %s` is a
/// real one. Every placeholder is a parameter, so the label is what remains once
/// they are taken out, and a proccode whose placeholder count is not the number
/// of arguments is not one raven-asm can write.
///
/// raven-asm can only write a parameter at the end (`name %s`), so the label is
/// trimmed: the separator space belongs to the label in Scratch's model but not
/// in raven-asm's, and leaving it on would send every ordinary custom block
/// through the name encoder.
fn proccode_parts(proccode: &str, arity: usize) -> Option<(String, Vec<ParamKind>)> {
    let mut label = String::with_capacity(proccode.len());
    let mut kinds = Vec::new();
    let mut rest = proccode;
    while let Some(at) = rest.find('%') {
        label.push_str(&rest[..at]);
        let mut chars = rest[at..].chars();
        chars.next();
        match chars.next() {
            Some('s') => kinds.push(ParamKind::String),
            Some('n') => kinds.push(ParamKind::Number),
            Some('b') => kinds.push(ParamKind::Boolean),
            // A `%` that introduces nothing is part of the name, and so is the
            // character after it.
            other => {
                label.push('%');
                if let Some(other) = other {
                    label.push(other);
                }
            }
        }
        rest = chars.as_str();
    }
    label.push_str(rest);
    if kinds.len() != arity {
        return None;
    }
    Some((label.trim().to_string(), kinds))
}

/// A mutation field that holds a JSON array encoded as a string.
fn string_list(value: Option<&Value>) -> Option<Vec<String>> {
    serde_json::from_str(value?.as_str()?).ok()
}

// ---------------------------------------------------------------------------
// Emitting one target
// ---------------------------------------------------------------------------

struct TargetPlan {
    is_stage: bool,
    name: String,
    vars: Vec<Sym>,
    lists: Vec<Sym>,
    costumes: Vec<ReadAsset>,
    sounds: Vec<ReadAsset>,
    blocks: BTreeMap<String, Value>,
    var_by_id: HashMap<String, String>,
    var_by_name: HashMap<String, String>,
    list_by_id: HashMap<String, String>,
    list_by_name: HashMap<String, String>,
    procs: HashMap<String, ProcPlan>,
}

/// The statements of one target, before the declarations go above them.
struct Rendered {
    declarations: String,
    body: String,
    blocks: usize,
    monitors: usize,
    warnings: Vec<Warning>,
    broadcasts: BTreeSet<String>,
}

fn render_target(plan: &TargetPlan, monitors: &HashMap<String, MonitorInfo>) -> Result<Rendered> {
    let mut renderer = Renderer {
        plan,
        body: String::new(),
        emitted: HashSet::new(),
        blocks: 0,
        detached: 0,
        params: HashMap::new(),
        warnings: Vec::new(),
        broadcasts: BTreeSet::new(),
    };
    renderer.roots()?;
    if renderer.detached > 0 {
        renderer.warnings.push(
            Warning::new(format!(
                "{} block(s) of `{}` are detached in the editor's workspace",
                renderer.detached, plan.name
            ))
            .note("they are not part of a script, so raven-asm does not write them")
            .note("a detached block never runs"),
        );
    }
    let (declarations, matched) = declarations(plan, monitors)?;
    Ok(Rendered {
        declarations,
        body: renderer.body,
        blocks: renderer.blocks,
        monitors: matched,
        warnings: renderer.warnings,
        broadcasts: renderer.broadcasts,
    })
}

/// The comment at the top of a target file.
fn header(plan: &TargetPlan) -> String {
    let mut out = format!(
        "// Reversed from a Scratch 3 project by {}.\n// One statement is one Scratch block, as everywhere in raven-asm.\n",
        crate::identity::DISPLAY
    );
    if plan
        .vars
        .iter()
        .chain(&plan.lists)
        .any(|sym| sym.rasm != sym.original)
    {
        out.push_str(&format!(
            "// A name raven-asm cannot write as an identifier became `{}` followed by its UTF-8 bytes in hex.\n",
            names::ENCODED_PREFIX
        ));
    }
    if !plan.is_stage {
        out.push_str("// This file owns only this sprite's variables and lists; the stage's are global to all of them.\n");
    }
    out.push('\n');
    out
}

/// The declarations at the top of a target file, and how many monitors matched.
fn declarations(
    plan: &TargetPlan,
    monitors: &HashMap<String, MonitorInfo>,
) -> Result<(String, usize)> {
    let mut out = String::new();
    let mut matched = 0usize;
    let owner = if plan.is_stage {
        ""
    } else {
        plan.name.as_str()
    };

    for sym in &plan.vars {
        let info = monitors
            .get(&sym.id)
            .or_else(|| monitors.get(&name_key(owner, &sym.original)));
        if info.is_some() {
            matched += 1;
        }
        renamed_note(&mut out, sym);
        out.push_str(&format!(
            "    {}{}var {} = {};{}\n",
            if info.is_some_and(|info| info.visible) {
                "visible "
            } else {
                ""
            },
            "",
            sym.rasm,
            literal(&sym.init)?,
            monitor_suffix(info, false)
        ));
    }
    for sym in &plan.lists {
        let info = monitors
            .get(&sym.id)
            .or_else(|| monitors.get(&name_key(owner, &sym.original)));
        if info.is_some() {
            matched += 1;
        }
        renamed_note(&mut out, sym);
        let items = match &sym.init {
            Value::Array(items) => {
                let items = items.iter().map(literal).collect::<Result<Vec<_>>>()?;
                wrap_items(&items)
            }
            Value::Null => String::new(),
            other => {
                return Err(Error::msg(format!(
                    "the list `{}` does not start with a list",
                    sym.original
                ))
                .note(other.to_string()))
            }
        };
        out.push_str(&format!(
            "    {}{}list {} = [{}];{}\n",
            if info.is_some_and(|info| info.visible) {
                "visible "
            } else {
                ""
            },
            "",
            sym.rasm,
            items,
            monitor_suffix(info, true)
        ));
    }
    for costume in &plan.costumes {
        let asset = format!("{}/{}", crate::identity::ASSET_DIR, costume.md5ext);
        let mut line = format!("    costume {} = {}", quote(&costume.name), quote(&asset));
        if let (Some(x), Some(y)) = (costume.rotation_x, costume.rotation_y) {
            line.push_str(&format!(" center {} {}", number(x), number(y)));
        }
        line.push_str(";\n");
        out.push_str(&line);
    }
    for sound in &plan.sounds {
        let asset = format!("{}/{}", crate::identity::ASSET_DIR, sound.md5ext);
        out.push_str(&format!(
            "    sound {} = {};\n",
            quote(&sound.name),
            quote(&asset)
        ));
    }
    out.push('\n');
    Ok((out, matched))
}

fn renamed_note(out: &mut String, sym: &Sym) {
    if sym.rasm != sym.original {
        out.push_str(&format!(
            "    // the project calls this {}; raven-asm cannot write that as an identifier\n",
            comment_text(&sym.original)
        ));
    }
}

fn monitor_suffix(info: Option<&MonitorInfo>, is_list: bool) -> String {
    let Some(info) = info else {
        return String::new();
    };
    let mut out = String::new();
    if let (Some(x), Some(y)) = (info.x, info.y) {
        out.push_str(&format!(" at {} {}", number(x), number(y)));
    }
    if !is_list {
        match info.mode.as_str() {
            "large" => out.push_str(" large"),
            "slider" => out.push_str(&format!(
                " slider {} {}",
                number(info.min.unwrap_or(0.0)),
                number(info.max.unwrap_or(100.0))
            )),
            _ => {}
        }
        if info.discrete == Some(false) {
            out.push_str(" continuous");
        }
    }
    out
}

/// A list's items, wrapped so a list with thousands of them stays a file an
/// editor and a diff can open. Whitespace between items means nothing to the
/// parser, so this changes nothing about what is written.
fn wrap_items(items: &[String]) -> String {
    const INDENT: &str = "        ";
    const WIDTH: usize = 88;
    let mut out = String::new();
    let mut column = 4 + INDENT.len();
    for (index, item) in items.iter().enumerate() {
        let width = item.chars().count();
        if index > 0 {
            out.push(',');
            column += 1;
            if column + 1 + width > WIDTH {
                out.push('\n');
                out.push_str(INDENT);
                column = INDENT.len();
            } else {
                out.push(' ');
                column += 1;
            }
        }
        out.push_str(item);
        column += width;
    }
    out
}

/// A literal value as raven-asm writes one.
fn literal(value: &Value) -> Result<String> {
    Ok(match value {
        Value::Null => "\"\"".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => quote(text),
        other => {
            return Err(
                Error::msg("a variable or list holds a value raven-asm cannot write")
                    .note(format!("the value is {}", other))
                    .note("Scratch stores only numbers, strings and booleans"),
            )
        }
    })
}

/// The original name, made safe to put inside a `//` comment.
fn comment_text(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    format!("`{cleaned}`")
}

/// A file name for one sprite, unique among the sprites of the project.
///
/// A sprite named in any script keeps its name: a letter or digit of any
/// alphabet is a letter or a digit, so `画字` becomes `画字.rasm` rather than a
/// row of underscores. Everything else — a space, punctuation, an emoji —
/// becomes `_`.
fn unique_slug(name: &str, used: &mut HashSet<String>) -> String {
    let mut base: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                '_'
            }
        })
        .collect();
    if base.is_empty() {
        base.push_str("sprite");
    }
    if used.insert(base.clone()) {
        return base;
    }
    let mut round = 2;
    loop {
        let candidate = format!("{base}{round}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        round += 1;
    }
}

fn manifest_text(name: &str, sprites: &[String], extensions: &[String]) -> String {
    let extensions: Vec<&String> = extensions
        .iter()
        .filter(|id| matches!(id.as_str(), "pen" | "music"))
        .collect();
    let name = sanitize(name);
    let out = format!(
        "# Written by {} from a Scratch 3 project.\n#\n#   raven-asm check     parse and validate without writing anything\n#   raven-asm build     compile this project into dist/{name}.sb3\n#\n# Paths in this file are relative to it.\n\n[project]\nname = {}\noutput = \"dist\"\nextensions = [{}]\n\n[targets]\nstage = {}\nsprites = [{}]\n",
        crate::identity::DISPLAY,
        quote(&name),
        extensions
            .iter()
            .map(|id| quote(id))
            .collect::<Vec<_>>()
            .join(", "),
        quote(crate::identity::STAGE_FILE),
        sprites
            .iter()
            .map(|path| quote(path))
            .collect::<Vec<_>>()
            .join(", ")
    );
    out
}

/// A project name that survives being written into a TOML string.
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c == '"' || c == '\\' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    if cleaned.is_empty() {
        "project".to_string()
    } else {
        cleaned
    }
}

/// The archive member a costume or sound needs.
fn require_asset<'a>(
    archive: &'a crate::zipr::Archive,
    md5ext: &str,
    name: &str,
    kind: &str,
) -> Result<&'a crate::zipr::Entry> {
    if md5ext.is_empty() {
        return Err(Error::msg(format!("the {kind} `{name}` has no file"))
            .note("every costume and sound names its archive member in `md5ext`"));
    }
    if md5ext.contains(['/', '\\']) || md5ext.contains("..") {
        return Err(Error::msg(format!(
            "the {kind} `{name}` names `{md5ext}`, which is not a file name"
        ))
        .note("an asset is written as `<md5>.<ext>`"));
    }
    archive.get(md5ext).ok_or_else(|| {
        Error::msg(format!(
            "the archive has no `{md5ext}`, which the {kind} `{name}` needs"
        ))
        .note("the `.sb3` is missing an asset it refers to")
    })
}

// ---------------------------------------------------------------------------
// The walk
// ---------------------------------------------------------------------------

struct Renderer<'a> {
    plan: &'a TargetPlan,
    body: String,
    emitted: HashSet<String>,
    blocks: usize,
    detached: usize,
    params: HashMap<String, String>,
    warnings: Vec<Warning>,
    broadcasts: BTreeSet<String>,
}

impl Renderer<'_> {
    /// Emit every script and every custom block of the target.
    fn roots(&mut self) -> Result<()> {
        let mut roots: Vec<(f64, f64, &String, &Value)> = Vec::new();
        for (id, block) in &self.plan.blocks {
            let Some(object) = block.as_object() else {
                continue;
            };
            let top = object.get("topLevel").and_then(Value::as_bool) == Some(true);
            let orphan = object.get("parent").is_none_or(|parent| parent.is_null());
            let shadow = object.get("shadow").and_then(Value::as_bool) == Some(true);
            if !top || !orphan || shadow || object.get("opcode").is_none() {
                continue;
            }
            let x = object.get("x").and_then(Value::as_f64).unwrap_or(0.0);
            let y = object.get("y").and_then(Value::as_f64).unwrap_or(0.0);
            roots.push((y, x, id, block));
        }
        // The editor's own reading order: down the workspace, then across.
        roots.sort_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then(a.1.total_cmp(&b.1))
                .then(a.2.cmp(b.2))
        });

        for (_, _, id, block) in roots {
            self.separate();
            let opcode = block_opcode(block)?;
            if opcode == "procedures_definition" {
                self.proc_block(id, block)?;
                continue;
            }
            if opcode == "procedures_call" {
                self.detached += 1;
                continue;
            }
            match catalog::block(opcode) {
                Some(spec) if spec.kind == BlockKind::Hat => {
                    self.emitted.insert(id.clone());
                    self.blocks += 1;
                    let args = self.args(spec, block, id)?;
                    let head = call_text(opcode, &args);
                    match block.get("next").and_then(Value::as_str) {
                        None => self.line(1, &format!("{head};")),
                        Some(next) => {
                            self.line(1, &format!("{head} {{"));
                            self.chain(Some(next), 2)?;
                            self.line(1, "}");
                        }
                    }
                }
                // A reporter or command block sitting loose in the workspace is
                // not a script; Scratch saves it, and it never runs.
                Some(_) => self.detached += 1,
                None => return Err(unknown(opcode)),
            }
        }
        Ok(())
    }

    fn proc_block(&mut self, id: &str, block: &Value) -> Result<()> {
        let plan = self.proc_of(block)?.clone();
        self.emitted.insert(id.to_string());
        self.blocks += 1;
        let params = plan
            .params
            .iter()
            .map(|(_, rasm, kind)| format!("{rasm}: {}", kind.spelling()))
            .collect::<Vec<_>>()
            .join(", ");
        let warp = if plan.warp { " warp" } else { "" };
        self.line(1, &format!("proc {}({params}){warp} {{", plan.name));
        let saved = std::mem::take(&mut self.params);
        self.params = plan
            .params
            .iter()
            .map(|(original, rasm, _)| (original.clone(), rasm.clone()))
            .collect();
        let body = block.get("next").and_then(Value::as_str);
        let result = self.chain(body, 2);
        self.params = saved;
        result?;
        self.line(1, "}");
        Ok(())
    }

    /// The plan of the custom block a `procedures_definition` declares.
    fn proc_of(&self, definition: &Value) -> Result<&ProcPlan> {
        let proccode = definition
            .get("inputs")
            .and_then(|inputs| inputs.get("custom_block"))
            .and_then(Value::as_array)
            .and_then(|input| input.get(1))
            .and_then(Value::as_str)
            .and_then(|prototype| self.plan.blocks.get(prototype))
            .and_then(|prototype| prototype.get("mutation"))
            .and_then(|mutation| mutation.get("proccode"))
            .and_then(Value::as_str)
            .ok_or_else(|| Error::msg("a custom block has no prototype proccode"))?;
        self.plan
            .procs
            .get(proccode)
            .ok_or_else(|| Error::msg(format!("the custom block `{proccode}` has no declaration")))
    }

    /// Emit a run of blocks joined by `next`.
    fn chain(&mut self, first: Option<&str>, depth: usize) -> Result<()> {
        let mut cursor = first.map(str::to_string);
        while let Some(id) = cursor {
            if !self.emitted.insert(id.clone()) {
                return Err(Error::msg(format!(
                    "the block `{id}` is used in more than one place"
                ))
                .note("a Scratch block sits in exactly one stack, so the project's block graph is not one the editor writes"));
            }
            let block = self.block(&id)?.clone();
            let opcode = block_opcode(&block)?.to_string();
            let next = block
                .get("next")
                .and_then(Value::as_str)
                .map(str::to_string);

            if opcode == "procedures_call" {
                self.blocks += 1;
                let call = self.proc_call(&block)?;
                self.line(depth, &format!("{call};"));
                cursor = next;
                continue;
            }

            let spec = catalog::block(&opcode).ok_or_else(|| unknown(&opcode))?;
            if spec.kind.is_value() {
                return Err(Error::msg(format!(
                    "`{opcode}` is a reporter block, so it cannot stand as a statement"
                ))
                .note("reporters produce values; raven-asm uses them as arguments"));
            }
            self.blocks += 1;
            let args = self.args(spec, &block, &id)?;
            let head = call_text(&opcode, &args);

            match spec.body {
                Body::None => {
                    self.line(depth, &format!("{head};"));
                    cursor = next;
                }
                // A hat's body, and the blocks below a `stop` that leaves a
                // notch, are the block's own `next`.
                Body::Next => {
                    if opcode == "control_stop" && next.is_some() && !self.stop_has_notch(&block) {
                        return Err(Error::msg(
                            "`control_stop` stops everything but the project has blocks below it",
                        )
                        .note("only `other scripts in sprite` and `other scripts in stage` leave a notch")
                        .note(CANNOT_WRITE));
                    }
                    match next {
                        None => self.line(depth, &format!("{head};")),
                        Some(next) => {
                            self.line(depth, &format!("{head} {{"));
                            self.chain(Some(&next), depth + 1)?;
                            self.line(depth, "}");
                        }
                    }
                    cursor = None;
                }
                Body::Substack | Body::SubstackElse => {
                    self.line(depth, &format!("{head} {{"));
                    self.chain(self.substack(&block, "SUBSTACK")?.as_deref(), depth + 1)?;
                    if spec.body == Body::SubstackElse {
                        self.line(depth, "} else {");
                        self.chain(self.substack(&block, "SUBSTACK2")?.as_deref(), depth + 1)?;
                    }
                    self.line(depth, "}");
                    cursor = next;
                }
            }
        }
        Ok(())
    }

    fn block(&self, id: &str) -> Result<&Value> {
        self.plan.blocks.get(id).ok_or_else(|| {
            Error::msg(format!(
                "the project refers to the block `{id}`, which it does not have"
            ))
        })
    }

    fn stop_has_notch(&self, block: &Value) -> bool {
        matches!(
            field_name(block, "STOP_OPTION"),
            Some("other scripts in sprite") | Some("other scripts in stage")
        )
    }

    fn substack(&self, block: &Value, name: &str) -> Result<Option<String>> {
        let Some(input) = block
            .get("inputs")
            .and_then(|inputs| inputs.get(name))
            .and_then(Value::as_array)
        else {
            return Ok(None);
        };
        if input.first().and_then(Value::as_u64) != Some(2) {
            return Err(Error::msg(format!(
                "the `{name}` of `{}` is not a stack",
                block_opcode(block).unwrap_or("?")
            ))
            .note(CANNOT_WRITE));
        }
        Ok(input.get(1).and_then(Value::as_str).map(str::to_string))
    }

    // -- calls ------------------------------------------------------------

    fn proc_call(&mut self, block: &Value) -> Result<String> {
        let proccode = block
            .get("mutation")
            .and_then(|mutation| mutation.get("proccode"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                Error::msg("the project has a custom block call with no proccode")
                    .note("Scratch identifies a custom block call by its proccode")
            })?;
        let plan = self.plan.procs.get(proccode).cloned().ok_or_else(|| {
            Error::msg(format!(
                "the project calls the custom block `{proccode}` but never defines it"
            ))
            .note("a Scratch custom block belongs to one target, and this one has no `proc`")
        })?;
        let mut args = Vec::new();
        for (index, arg_id) in plan.arg_ids.iter().enumerate() {
            let kind = plan
                .params
                .get(index)
                .map(|(_, _, kind)| *kind)
                .unwrap_or(ParamKind::String);
            let shape = match kind {
                ParamKind::Boolean => Shape::Bool,
                ParamKind::Number => Shape::Number,
                ParamKind::String => Shape::Text,
            };
            let value = block
                .get("inputs")
                .and_then(|inputs| inputs.get(arg_id))
                .cloned();
            args.push(match value {
                Some(value) => {
                    let what = format!("the `procedures_call` argument `{arg_id}`");
                    self.input(shape, &value, &what)?
                }
                None => self.missing(shape, block_opcode(block)?)?,
            });
        }
        Ok(call_text(&plan.name, &args))
    }

    /// Every argument of a block, in the catalog's order.
    fn args(&mut self, spec: &BlockSpec, block: &Value, id: &str) -> Result<Vec<String>> {
        let mut out = Vec::with_capacity(spec.args.len());
        for arg in spec.args {
            out.push(match (arg.wire, arg.shape) {
                (Wire::Field, Shape::Variable) => {
                    let name = field_name(block, arg.name).unwrap_or_default().to_string();
                    let var_id = field_id(block, arg.name);
                    quote(&self.variable(var_id, &name)?)
                }
                (Wire::Field, Shape::List) => {
                    let name = field_name(block, arg.name).unwrap_or_default().to_string();
                    let list_id = field_id(block, arg.name);
                    quote(&self.list(list_id, &name)?)
                }
                (Wire::Field, Shape::Broadcast) => {
                    let name = field_name(block, arg.name).unwrap_or_default().to_string();
                    self.broadcasts.insert(name.clone());
                    quote(&name)
                }
                (Wire::Field, Shape::ParamName) => {
                    let name = field_name(block, arg.name)
                        .ok_or_else(|| Error::msg("a parameter reporter has no parameter name"))?
                        .to_string();
                    quote(&self.parameter(&name)?)
                }
                (Wire::Field, Shape::Menu(_)) => {
                    let value = field_name(block, arg.name).ok_or_else(|| {
                        Error::msg(format!(
                            "the block `{}` has no `{}` value",
                            block_opcode(block).unwrap_or("?"),
                            arg.name
                        ))
                        .note(CANNOT_WRITE)
                    })?;
                    quote(value)
                }
                (Wire::Field, shape) => {
                    return Err(Error::msg(format!(
                        "internal error: `{}` holds `{shape:?}` as a field",
                        block_opcode(block).unwrap_or("?"),
                    )))
                }
                (Wire::Input, shape) => {
                    let value = block
                        .get("inputs")
                        .and_then(|inputs| inputs.get(arg.name))
                        .cloned();
                    let what = format!(
                        "the `{}` input `{}`",
                        block_opcode(block).unwrap_or("?"),
                        arg.name
                    );
                    match value {
                        Some(value) => self.input(shape, &value, &what)?,
                        None => self.missing(shape, id)?,
                    }
                }
            });
        }
        Ok(out)
    }

    /// One input descriptor: `[1, primitive]`, `[1, shadowId]`, `[2, blockId]`
    /// or `[3, blockId, shadow]`.
    ///
    /// The Scratch serializer compresses a variable, list or broadcast reporter
    /// that stands alone in an input into its primitive form in place, so the
    /// second element can be `[12, "score", "id"]` rather than a block id. The
    /// compressed form is the value either way, and the shadow behind it is not.
    ///
    /// `what` names the input in a diagnostic, because a project can carry an
    /// input shape the editor never writes and the report has to say where.
    fn input(&mut self, shape: Shape, descriptor: &Value, what: &str) -> Result<String> {
        let Some(parts) = descriptor.as_array() else {
            return Err(
                Error::msg(format!("{what} is `{descriptor}`, not an input"))
                    .note("an input is an array such as `[1, [10, \"text\"]]`"),
            );
        };
        let tag = parts.first().and_then(Value::as_u64);
        match (tag, parts.get(1)) {
            (Some(1..=3), Some(Value::Array(primitive))) => self.primitive(shape, primitive, what),
            (Some(1), Some(Value::String(shadow))) => self.shadow(shape, shadow),
            (Some(2) | Some(3), Some(Value::String(block))) => self.reporter(block),
            _ => Err(Error::msg(format!(
                "{what} is `{descriptor}`, which is not an input the Scratch editor writes"
            ))
            .note(CANNOT_WRITE)),
        }
    }

    /// A value stored inline rather than as a block.
    ///
    /// The primitive tags are Scratch's `primitiveOpcodeInfoMap`: 4–8 a number
    /// of some shape, 9 a colour, 10 text, 11 a broadcast, 12 a variable, 13 a
    /// list.
    ///
    /// Only an input whose own shape is `Variable`, `List` or `Broadcast` is
    /// written as a bare string, because that is how the compiler stores one.
    /// Every other input takes a reporter, so a variable that was compressed
    /// into `[12, "score", "id"]` is written back as `data_variable("score")` —
    /// writing the bare name there would make it a string literal, which reads
    /// as a value rather than as the variable.
    fn primitive(&mut self, shape: Shape, primitive: &[Value], what: &str) -> Result<String> {
        let tag = primitive.first().and_then(Value::as_u64);
        let value = primitive.get(1).cloned().unwrap_or(Value::Null);
        match tag {
            // math_number, math_positive_number, math_whole_number,
            // math_integer, math_angle
            Some(4..=8) => Ok(literal(&value)?),
            Some(9) => {
                let text = value.as_str().unwrap_or_default();
                if !is_color(text) {
                    return Err(Error::msg(format!("`{text}` is not a colour"))
                        .note("a colour input is written `#rrggbb`"));
                }
                Ok(quote(text))
            }
            Some(10) => Ok(literal(&value)?),
            Some(11) => {
                let name = value.as_str().unwrap_or_default().to_string();
                self.broadcasts.insert(name.clone());
                match shape {
                    Shape::Broadcast => Ok(quote(&name)),
                    _ => Err(Error::msg(format!(
                        "{what} holds the broadcast `{name}`, which raven-asm can only read in a broadcast input"
                    ))
                    .note(CANNOT_WRITE)),
                }
            }
            Some(12) => {
                let name = primitive.get(1).and_then(Value::as_str).unwrap_or_default();
                let var_id = primitive.get(2).and_then(Value::as_str);
                let rasm = self.variable(var_id, name)?;
                match shape {
                    Shape::Variable => Ok(quote(&rasm)),
                    Shape::Bool => Err(Error::msg(format!(
                        "{what} holds the variable `{name}`, but raven-asm's boolean inputs take a condition block"
                    ))
                    .note(CANNOT_WRITE)),
                    _ => Ok(format!("data_variable({})", quote(&rasm))),
                }
            }
            Some(13) => {
                let name = primitive.get(1).and_then(Value::as_str).unwrap_or_default();
                let list_id = primitive.get(2).and_then(Value::as_str);
                let rasm = self.list(list_id, name)?;
                match shape {
                    Shape::List => Ok(quote(&rasm)),
                    _ => Ok(format!("data_listcontents({})", quote(&rasm))),
                }
            }
            other => Err(Error::msg(format!(
                "{what} uses the primitive {other:?} (`{shape:?}`), which is not one Scratch writes"
            ))
            .note(CANNOT_WRITE)),
        }
    }

    /// A dropdown whose value sits in a shadow menu block.
    fn shadow(&mut self, shape: Shape, shadow_id: &str) -> Result<String> {
        let Shape::Menu(menu_id) = shape else {
            return Err(Error::msg(
                "an input points at a shadow block, which only a dropdown does",
            )
            .note(CANNOT_WRITE));
        };
        let spec = catalog::menu(menu_id)
            .ok_or_else(|| Error::msg(format!("internal error: no catalog menu `{menu_id}`")))?;
        let shadow = self.block(shadow_id)?;
        let value = field_name(shadow, spec.field).ok_or_else(|| {
            Error::msg(format!(
                "the dropdown `{}` has no `{}` value",
                spec.opcode, spec.field
            ))
            .note(CANNOT_WRITE)
        })?;
        Ok(quote(value))
    }

    /// A reporter block used as a value.
    fn reporter(&mut self, id: &str) -> Result<String> {
        let block = self.block(id)?.clone();
        let opcode = block_opcode(&block)?.to_string();
        let spec = catalog::block(&opcode).ok_or_else(|| unknown(&opcode))?;
        if !spec.kind.is_value() {
            return Err(Error::msg(format!("`{opcode}` does not produce a value"))
                .note("only reporter and boolean blocks fill an input"));
        }
        let args = self.args(spec, &block, id)?;
        // A reporter is always written with parentheses: a bare identifier in a
        // value position is not a value to the raven-asm parser.
        Ok(format!("{opcode}({})", args.join(", ")))
    }

    fn variable(&self, id: Option<&str>, name: &str) -> Result<String> {
        if let Some(id) = id {
            if let Some(rasm) = self.plan.var_by_id.get(id) {
                return Ok(rasm.clone());
            }
        }
        self.plan.var_by_name.get(name).cloned().ok_or_else(|| {
            Error::msg(format!(
                "the project reads the variable `{name}`, which it does not declare"
            ))
            .note("a vanilla Scratch project declares every variable it uses")
        })
    }

    fn list(&self, id: Option<&str>, name: &str) -> Result<String> {
        if let Some(id) = id {
            if let Some(rasm) = self.plan.list_by_id.get(id) {
                return Ok(rasm.clone());
            }
        }
        self.plan.list_by_name.get(name).cloned().ok_or_else(|| {
            Error::msg(format!(
                "the project reads the list `{name}`, which it does not declare"
            ))
            .note("a vanilla Scratch project declares every list it uses")
        })
    }

    fn parameter(&self, name: &str) -> Result<String> {
        self.params.get(name).cloned().ok_or_else(|| {
            Error::msg(format!(
                "the project reads the parameter `{name}` outside the custom block that declares it"
            ))
            .note("`argument_reporter_string_number` and `argument_reporter_boolean` belong inside their own `proc`")
        })
    }

    /// An input the editor left empty.
    fn missing(&mut self, shape: Shape, opcode: &str) -> Result<String> {
        match shape {
            Shape::Number | Shape::Positive | Shape::Whole | Shape::Integer | Shape::Angle => {
                self.warnings.push(
                    Warning::new(format!("`{opcode}` has an empty number input"))
                        .note("an empty slot reads as 0, which is what was written")
                        .note(CANNOT_WRITE),
                );
                Ok("0".to_string())
            }
            Shape::Text => {
                self.warnings.push(
                    Warning::new(format!("`{opcode}` has an empty text input"))
                        .note("an empty slot reads as \"\", which is what was written")
                        .note(CANNOT_WRITE),
                );
                Ok("\"\"".to_string())
            }
            // An empty hexagonal slot is a condition that reads as false.
            Shape::Bool => {
                self.warnings.push(
                    Warning::new(format!("`{opcode}` has an empty boolean input"))
                        .note("an empty condition reads as false, so `operator_equals(1, 2)` was written")
                        .note(CANNOT_WRITE),
                );
                Ok("operator_equals(1, 2)".to_string())
            }
            other => Err(Error::msg(format!(
                "`{opcode}` has no value for a required input ({other:?})"
            ))
            .note(CANNOT_WRITE)),
        }
    }

    fn line(&mut self, depth: usize, text: &str) {
        for _ in 0..depth {
            self.body.push_str("    ");
        }
        self.body.push_str(text);
        self.body.push('\n');
    }

    /// An empty line between two top-level items, so scripts and custom blocks
    /// read apart from each other.
    fn separate(&mut self) {
        if !self.body.is_empty() && !self.body.ends_with("\n\n") {
            self.body.push('\n');
        }
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn unknown(opcode: &str) -> Error {
    let mut error =
        Error::msg(format!("`{opcode}` is not a block vanilla Scratch 3 has")).note(NOT_SUPPORTED);
    if let Some(hint) = catalog::suggest(opcode) {
        error = error.note(format!("did you mean `{hint}`?"));
    }
    error
}

/// `project.json` is missing: say whether the archive is empty or wraps
/// everything in a folder, which is what re-zipping a `.sb3` by hand produces.
fn missing_project(archive: &crate::zipr::Archive) -> Error {
    let error = Error::msg("the archive has no `project.json`")
        .note("a `.sb3` holds `project.json` and one file per costume and sound");
    match archive
        .entries
        .iter()
        .find(|entry| entry.name.ends_with("/project.json"))
    {
        Some(entry) => error
            .note(format!("this archive holds `{}` instead", entry.name))
            .note("the project was archived inside a folder; a `.sb3` keeps its files at the root"),
        None => error,
    }
}

fn call_text(name: &str, args: &[String]) -> String {
    if args.is_empty() {
        name.to_string()
    } else {
        format!("{name}({})", args.join(", "))
    }
}

fn block_opcode(block: &Value) -> Result<&str> {
    block
        .get("opcode")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::msg("a block has no opcode").note("every Scratch block has one"))
}

fn field_name<'a>(block: &'a Value, name: &str) -> Option<&'a str> {
    block
        .get("fields")?
        .get(name)?
        .as_array()?
        .first()?
        .as_str()
}

fn field_id<'a>(block: &'a Value, name: &str) -> Option<&'a str> {
    block.get("fields")?.get(name)?.as_array()?.get(1)?.as_str()
}

fn is_color(text: &str) -> bool {
    let Some(hex) = text.strip_prefix('#') else {
        return false;
    };
    hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proccodes_split_into_a_label_and_kinds() {
        let (label, kinds) = proccode_parts("go_to %n %n", 2).unwrap();
        assert_eq!(label, "go_to");
        assert_eq!(kinds, vec![ParamKind::Number, ParamKind::Number]);

        let (label, kinds) = proccode_parts("hop", 0).unwrap();
        assert_eq!(label, "hop");
        assert!(kinds.is_empty());

        let (label, kinds) = proccode_parts("say %s %b", 2).unwrap();
        assert_eq!(label, "say");
        assert_eq!(kinds, vec![ParamKind::String, ParamKind::Boolean]);

        // A parameter may sit anywhere in the name, which is how a real project
        // spells one: `找字 %s 大写: %s`.
        let (label, kinds) = proccode_parts("找字 %s 大写: %s", 2).unwrap();
        assert_eq!(label, "找字  大写:");
        assert_eq!(kinds, vec![ParamKind::String, ParamKind::String]);

        // A percent that introduces nothing is part of the name.
        let (label, kinds) = proccode_parts("100% sure", 0).unwrap();
        assert_eq!(label, "100% sure");
        assert!(kinds.is_empty());

        assert!(proccode_parts("say %s", 2).is_none());
        assert!(proccode_parts("say %s", 1).is_some());
    }

    #[test]
    fn slugs_are_stable_and_unique() {
        let mut used = HashSet::new();
        assert_eq!(unique_slug("Sprite1", &mut used), "sprite1");
        assert_eq!(unique_slug("Sprite1", &mut used), "sprite12");
        assert_eq!(unique_slug("", &mut used), "sprite");
    }
}
