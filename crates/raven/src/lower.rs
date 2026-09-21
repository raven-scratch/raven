//! Elaboration: names, types, macros and the descent to raven-asm.
//!
//! This is the whole front end, in one traversal, and it is one traversal on
//! purpose. Macro expansion and type checking cannot be separated: a macro
//! argument's type decides whether an expansion is legal, and a macro body's
//! names resolve at the call site, so there is no point at which "the program
//! before expansion" is a checkable thing. What *is* kept separate is the
//! artifact — [`crate::rasm`] — which `raven expand` prints and which the build
//! hands straight back to `raven-asm`.
//!
//! The rules this implements, in the order they matter:
//!
//! * a name resolves against the target's own items, then the public items of
//!   every module it uses, then the intrinsic modules, then the prelude;
//! * a macro parameter of kind `expr` is substituted, so a parameter used more
//!   than once across statements must have a pure argument
//!   ([`crate::purity`]);
//! * a name a macro introduces is renamed per expansion, so two expansions of
//!   the same macro cannot collide;
//! * macro expansion is acyclic, and a cycle is reported with the whole chain.
//!
//! # The virtual memory system
//!
//! Every raven local — a `let`, a `for` counter, a procedure's return value —
//! lives in one per-target list, `_vms`, addressed by a compile-time constant
//! 1-based cell index. A cell belongs to its **declaration site**, not to a
//! call: it is allocated once, the same way for every run, and every execution
//! of the code that reads it sees the same cell. That is what makes the emitted
//! project a fixed tree of blocks, and it is why a recursive `proc` may not
//! declare a `let` at all — every level of the recursion would share the one
//! cell, and the program would quietly read another level's value. [`Unit`]
//! builds the procedure call graph and refuses such a `let`; a recursive `proc`
//! must pass values as parameters, keep them in a `var`, or recurse without
//! block-local storage.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use raven_asm::ast as rasm;
use raven_scratch::catalog::{self, BlockSpec, Shape, Wire};
use raven_scratch::diag::{Diag, Error, Pos, Result, Source};

use crate::ast::{self, Block, Expr, Ident, Item, MacroArg, MacroBody, Stmt};
use crate::diag::Span;
use crate::menu;
use crate::module::{Program, TargetPlan, Unit as FileUnit};
use crate::purity::{self, Purity};
use crate::rasm::{
    block as mk_block, block_else as mk_block_else, boolean as mk_bool, call as mk_call,
    num as mk_num, stmt as mk_stmt, str_ as mk_str,
};
use crate::stdlib::{self, Binding, Value};
use crate::ty::{self, Scalar, Ty};

/// The prelude, written in raven. It is a normal module: every item in it is an
/// ordinary macro that a program may read, and shadow.
const PRELUDE: &str = include_str!("prelude.rav");

/// The name of the per-target list every raven local lives in.
const VMS: &str = "_vms";

/// The name of the project-wide list the stage declares for global state.
const GLOBAL_VM: &str = "_gvm";

/// The memory manager's two procedures: `warp` blocks that grow an arena to
/// what the program can use and put the declared starting values in place.
const RESERVE_VMS: &str = "__vms_reserve";
const RESERVE_GVM: &str = "__gvm_reserve";

/// The name of the console list: every log line is one item of it.
const CONSOLE: &str = "_console";

/// The module the console's three functions live in. It is not a Scratch
/// block, so it is not in the catalog: `console::log` is one `add` to
/// [`CONSOLE`].
const CONSOLE_MODULE: &str = "console";

/// The prefix of the per-script stacks, one per script that keeps block-scoped
/// state: `_stack0`, `_stack1`, …
const STACK: &str = "_stack";

/// How many cells one struct may occupy.
///
/// A struct is a run of cells in one arena list, and the arena is declared with
/// one item per cell. A bound is what keeps a typo from declaring a struct that
/// makes a project's `project.json` enormous.
const MAX_STRUCT_SIZE: usize = 64;

/// How deep macro expansion may nest before it is refused.
const MAX_EXPANSION_DEPTH: usize = 32;

/// What a compiled project is: one raven-asm file per target.
#[derive(Debug)]
pub struct Output {
    pub files: Vec<rasm::File>,
    pub warnings: Vec<Diag>,
}

/// Compile every target.
pub fn compile(program: &Program) -> Result<Output> {
    let globals = Globals::collect(program)?;
    let prelude = Prelude::load()?;
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    for plan in &program.targets {
        let mut unit = Unit::new(program, plan, &globals, &prelude)?;
        let mut file = unit.run()?;
        if plan.kind == ast::TargetKind::Stage {
            unit.emit_globals(&mut file);
        }
        warnings.extend(unit.warnings.iter().cloned());
        files.push(file);
    }
    Ok(Output { files, warnings })
}

// ---------------------------------------------------------------------------
// Project-wide declarations
// ---------------------------------------------------------------------------

/// A variable or list, and where it lives.
///
/// A scalar is not a Scratch variable: it is a **cell** in the virtual memory
/// system, addressed by the constant index in [`VarInfo::cell`]. A list is a
/// Scratch list, because that is the only container Scratch gives the blocks a
/// language can drive one item at a time.
#[derive(Clone, Debug)]
struct VarInfo {
    name: String,
    ty: Ty,
    /// The starting value of a scalar, already in raven-asm form.
    init: rasm::Literal,
    /// The starting items of a list, one per item: Scratch keeps a list's value
    /// as an array, not as one string.
    items: Vec<rasm::Literal>,
    /// A list, as opposed to a scalar variable.
    is_list: bool,
    /// `true` when a list was declared `[]` rather than with items.
    declared_empty: bool,
    /// `true` when the variable belongs to the stage.
    global: bool,
    /// The cell a scalar lives in, 1-based. Zero for a list.
    cell: usize,
    /// Where the declaration is, so generated raven-asm can point back at it.
    pos: Pos,
}

impl VarInfo {
    /// The Scratch list a raven `list<T>` is backed by.
    fn as_list_item(&self) -> Option<rasm::Item> {
        if !self.is_list {
            return None;
        }
        // A list declared `[]` starts empty; one with items starts with them.
        let init = if self.declared_empty {
            Vec::new()
        } else {
            self.items.clone()
        };
        Some(rasm::Item::List(rasm::ListDecl {
            global: self.global,
            visible: false,
            monitor: rasm::MonitorSpec::default(),
            name: self.name.clone(),
            init,
            pos: self.pos,
        }))
    }
}

/// Everything that belongs to the project rather than to one target.
#[derive(Debug, Default)]
pub struct Globals {
    vars: Vec<VarInfo>,
    /// Every name `watch`ed anywhere in the project, so a *sprite* that writes a
    /// watched project-wide cell keeps its mirror in step too.
    watches: HashSet<String>,
    /// Whether anything anywhere logs to the console. Nothing that is not used
    /// is emitted, and that includes the console itself.
    console: bool,
    /// How many cells the project-wide arena, [`GLOBAL_VM`], holds.
    arena: usize,
    broadcasts: Vec<String>,
    stage_costumes: Vec<String>,
    stage_sounds: Vec<String>,
    sprite_names: Vec<String>,
}

impl Globals {
    fn collect(program: &Program) -> Result<Self> {
        let mut globals = Globals::default();
        let mut seen: HashMap<String, PathBuf> = HashMap::new();

        let mut visit = |unit: &FileUnit, globals: &mut Globals| -> Result<()> {
            let is_stage = unit
                .file
                .target()
                .is_some_and(|t| t.kind == ast::TargetKind::Stage);
            for item in items_of(unit) {
                match &item {
                    Item::Var(var) if var.public || is_stage => {
                        if let Some(error) = reserved(&var.name.name, var.name.span, &unit.source) {
                            return Err(error);
                        }
                        if let Some(first) = seen.get(&var.name.name) {
                            return Err(Error::new(unit.source.error(
                                var.name.span.pos,
                                format!("`{}` is declared project-wide more than once", var.name.name),
                            ))
                            .note(format!(
                                "the first declaration is in {}",
                                short(first)
                            ))
                            .note("a project-wide declaration lives on the stage, so there can only be one"));
                        }
                        seen.insert(var.name.name.clone(), unit.path.clone());
                        let mut info = var_info(var, true)?;
                        if info.ty.is_place() {
                            return Err(Error::new(
                                unit.source
                                    .error(
                                        var.name.span.pos,
                                        format!(
                                            "`{}` is a struct, which belongs to one target",
                                            var.name.name
                                        ),
                                    )
                                    .span(var.name.span.len.max(1))
                                    .note("a struct is a frame of cells in one target's VMS")
                                    .note("declare it inside the `sprite` or `stage` block, without `pub`"),
                            ));
                        }
                        if !info.is_list {
                            // A project-wide scalar is a cell in the project's
                            // arena; the index is the same in every target.
                            globals.arena += 1;
                            info.cell = globals.arena;
                        }
                        globals.vars.push(info);
                    }
                    Item::Watch(watch) => {
                        for name in &watch.names {
                            globals.watches.insert(name.name.clone());
                        }
                    }
                    Item::Broadcast(broadcast) => {
                        if !globals.broadcasts.contains(&broadcast.name) {
                            globals.broadcasts.push(broadcast.name.clone());
                        }
                    }
                    Item::Costume(costume) if is_stage => {
                        globals.stage_costumes.push(costume.name.clone());
                    }
                    Item::Sound(sound) if is_stage => {
                        globals.stage_sounds.push(sound.name.clone());
                    }
                    _ => {}
                }
            }
            Ok(())
        };

        for plan in &program.targets {
            visit(&plan.main, &mut globals)?;
            globals.console |= items_use_console(&items_of(&plan.main));
        }
        for unit in program.modules() {
            visit(unit, &mut globals)?;
            globals.console |= items_use_console(&items_of(unit));
        }
        for plan in &program.targets {
            if plan.kind == ast::TargetKind::Sprite {
                globals.sprite_names.push(plan.name.clone());
            }
        }
        Ok(globals)
    }

    fn var(&self, name: &str) -> Option<&VarInfo> {
        self.vars.iter().find(|v| v.name == name)
    }

    /// The project-wide cell a global scalar lives in, if there is one.
    fn scalar_cell(&self, name: &str) -> Option<usize> {
        self.vars
            .iter()
            .find(|v| v.name == name && !v.is_list)
            .map(|v| v.cell)
    }
}

/// Whether any of these items logs to the console.
fn items_use_console(items: &[Item]) -> bool {
    items.iter().any(|item| match item {
        Item::Target(target) => items_use_console(&target.items),
        Item::Script(script) => {
            stmts_use_console(&script.body) || script.hat.args.iter().any(expr_uses_console)
        }
        Item::Proc(proc) => stmts_use_console(&proc.body),
        Item::Fn(function) => expr_uses_console(&function.body),
        Item::Macro(decl) => match &decl.body {
            MacroBody::Expr(expr) => expr_uses_console(expr),
            MacroBody::Stmts(stmts) => stmts_use_console(stmts),
        },
        _ => false,
    })
}

fn stmts_use_console(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|stmt| match stmt {
        Stmt::Let(decl) => expr_uses_console(&decl.value),
        Stmt::Var(decl) => match &decl.init {
            // A declaration starts from literals, which cannot call anything.
            ast::Initializer::Value(_) | ast::Initializer::Items(_) => false,
            ast::Initializer::Fields(fields) => fields.iter().any(|(_, e)| expr_uses_console(e)),
        },
        Stmt::Assign(assign) => expr_uses_console(&assign.value),
        Stmt::CompoundAssign(assign) => expr_uses_console(&assign.value),
        Stmt::Return(ret) => ret.value.as_ref().is_some_and(expr_uses_console),
        Stmt::Call(call) => {
            call_is_console(&call.callee)
                || call.args.iter().any(expr_uses_console)
                || call.body.as_ref().is_some_and(|b| stmts_use_console(b))
        }
        Stmt::Method(stmt) => stmt.args.iter().any(expr_uses_console),
        Stmt::If(stmt) => {
            expr_uses_console(&stmt.cond)
                || stmts_use_console(&stmt.then_branch)
                || stmt
                    .else_branch
                    .as_ref()
                    .is_some_and(|b| stmts_use_console(b))
        }
        Stmt::Loop(stmt) => {
            let counted = match &stmt.kind {
                ast::LoopKind::Repeat(e) | ast::LoopKind::RepeatUntil(e) => expr_uses_console(e),
                ast::LoopKind::Forever => false,
            };
            counted || stmts_use_console(&stmt.body)
        }
        Stmt::Match(stmt) => {
            expr_uses_console(&stmt.subject)
                || stmt.arms.iter().any(|arm| stmts_use_console(&arm.body))
        }
        Stmt::Macro(call) => call.args.iter().any(macro_arg_uses_console),
        Stmt::Param(_) => false,
    })
}

fn macro_arg_uses_console(arg: &MacroArg) -> bool {
    match arg {
        MacroArg::Expr(expr) => expr_uses_console(expr),
        MacroArg::Block(stmts) => stmts_use_console(stmts),
        MacroArg::Ident(_) | MacroArg::Param(_) => false,
    }
}

fn expr_uses_console(expr: &Expr) -> bool {
    match expr {
        Expr::Call(call) => {
            call_is_console(&call.callee) || call.args.iter().any(expr_uses_console)
        }
        Expr::Index { list, index, .. } => expr_uses_console(list) || expr_uses_console(index),
        Expr::Field { base, .. } => expr_uses_console(base),
        Expr::Method { receiver, args, .. } => {
            expr_uses_console(receiver) || args.iter().any(expr_uses_console)
        }
        Expr::Struct { fields, .. } => fields.iter().any(|(_, e)| expr_uses_console(e)),
        Expr::Unary { operand, .. } => expr_uses_console(operand),
        Expr::Binary { lhs, rhs, .. } => expr_uses_console(lhs) || expr_uses_console(rhs),
        Expr::Interpolated { parts, .. } => parts.iter().any(|part| match part {
            ast::InterpPart::Text(_) => false,
            ast::InterpPart::Hole(expr) => expr_uses_console(expr),
        }),
        Expr::Macro(call) => call.args.iter().any(macro_arg_uses_console),
        _ => false,
    }
}

/// `console::log`, `console::clear` or `console::count`.
fn call_is_console(path: &ast::Path) -> bool {
    let names: Vec<&str> = path.segments.iter().map(|s| s.name.as_str()).collect();
    matches!(names.as_slice(), ["console", _] | ["std", "console", _])
}

/// Every item a file declares, whether at the top level or inside a target.
///
/// A file that declares a target may still declare items beside it — a
/// `struct`, or a `proc` shared by the target's scripts — and they belong to
/// that target. Before this, they were parsed and then silently ignored.
fn items_of(unit: &FileUnit) -> Vec<Item> {
    match unit.file.target() {
        Some(target) => unit
            .file
            .items
            .iter()
            .filter(|item| !matches!(item, Item::Target(_)))
            .cloned()
            .chain(target.items.iter().cloned())
            .collect(),
        None => unit.file.items.clone(),
    }
}

fn short(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// A user declaration of the name the virtual memory system lives under.
fn reserved(name: &str, span: Span, source: &Source) -> Option<Error> {
    if name == VMS || name == GLOBAL_VM {
        Some(Error::new(
            source
                .error(
                    span.pos,
                    format!("`{name}` is reserved for the virtual memory system"),
                )
                .span(span.len.max(1))
                .note("every variable, `let`, `for` counter and procedure return value lives in it; pick another name"),
        ))
    } else {
        None
    }
}

fn var_info(var: &ast::VarDecl, global: bool) -> Result<VarInfo> {
    let is_list = var.ty.is_list();
    // A struct is a place, not a value: it has no single starting value, so it
    // is laid out by the caller and this entry only carries its type.
    if var.ty.is_place() {
        return Ok(VarInfo {
            name: var.name.name.clone(),
            ty: var.ty,
            init: rasm::Literal::Str(String::new()),
            items: Vec::new(),
            is_list: false,
            declared_empty: false,
            global,
            cell: 0,
            pos: var.span.pos,
        });
    }
    let mut items = Vec::new();
    let init = match (&var.init, is_list) {
        (ast::Initializer::Value(literal), false) => to_literal(literal),
        (ast::Initializer::Items(list_items), true) => {
            items = list_items.iter().map(to_literal).collect();
            rasm::Literal::Str(String::new())
        }
        (ast::Initializer::Fields(_), _) => {
            return Err(Error::msg(format!(
                "`{}` is not a struct, so it cannot be built from fields",
                var.name.name
            )))
        }
        (ast::Initializer::Value(_), true) => {
            return Err(Error::msg(format!(
                "`{}` is a list, so it needs `= []`",
                var.name.name
            )))
        }
        (ast::Initializer::Items(_), false) => {
            return Err(Error::msg(format!(
                "`{}` is a scalar, so it takes one value rather than a list",
                var.name.name
            )))
        }
    };
    Ok(VarInfo {
        name: var.name.name.clone(),
        ty: var.ty,
        init,
        items,
        is_list,
        declared_empty: is_list && var.init_items_empty(),
        global,
        cell: 0,
        pos: var.span.pos,
    })
}

fn to_literal(literal: &ast::Literal) -> rasm::Literal {
    match &literal.kind {
        ast::LiteralKind::Number(text) => rasm::Literal::Number(text.clone()),
        ast::LiteralKind::Str(text) => rasm::Literal::Str(text.clone()),
        ast::LiteralKind::Bool(value) => rasm::Literal::Bool(*value),
    }
}

/// `item (cell) of _vms` — the target's own arena.
fn cell_read(cell: usize) -> rasm::Expr {
    arena_read(cell, VMS)
}

/// `replace item (cell) of _vms with value`.
fn cell_write(cell: usize, value: rasm::Expr) -> rasm::Stmt {
    arena_write(cell, VMS, value)
}

/// `item (cell) of _gvm` — the project arena on the stage.
fn gcell_read(cell: usize) -> rasm::Expr {
    arena_read(cell, GLOBAL_VM)
}

/// `item (cell) of <list>`.
fn arena_read(cell: usize, list: &str) -> rasm::Expr {
    mk_call(
        "data_itemoflist",
        vec![mk_num(cell.to_string()), mk_str(list)],
    )
}

/// `replace item (cell) of <list> with value`.
fn arena_write(cell: usize, list: &str, value: rasm::Expr) -> rasm::Stmt {
    mk_stmt(
        "data_replaceitemoflist",
        vec![mk_num(cell.to_string()), mk_str(list), value],
    )
}

/// `delete all of <list>` — a fresh stack for a fresh run.
fn arena_clear(list: &str) -> rasm::Stmt {
    mk_stmt("data_deletealloflist", vec![mk_str(list)])
}

/// A raven-asm literal as the expression a block input wants.
fn literal_of_rasm(literal: &rasm::Literal) -> rasm::Expr {
    match literal {
        rasm::Literal::Number(text) => mk_num(text.clone()),
        rasm::Literal::Str(text) => mk_str(text.clone()),
        rasm::Literal::Bool(value) => bool_literal(*value),
    }
}

/// `add value to <list>` — the *push* half of the stack.
fn arena_push(list: &str, value: rasm::Expr) -> rasm::Stmt {
    mk_stmt("data_addtolist", vec![value, mk_str(list)])
}

/// `delete (cell) of <list>` — the *pop* half of the stack.
///
/// The cell is the one the popped value was pushed at, which is the top of the
/// stack by construction, so a plain index is enough and no length lookup is
/// needed.
fn arena_pop(cell: usize, list: &str) -> rasm::Stmt {
    mk_stmt(
        "data_deleteoflist",
        vec![mk_num(cell.to_string()), mk_str(list)],
    )
}

/// `if (length of <list>) < (index) { repeat until it is not { add "" } }`.
///
/// Scratch's `replace item` does nothing at all when the list is shorter than
/// the index, which would make `xs[i] = v` on a short list a silent no-op: the
/// write lands nowhere and the read that follows returns the empty string.
/// Growing the list to the index is what makes a raven list an array that is as
/// long as it is used, and no longer.
fn list_grow(list: &str, index: rasm::Expr) -> rasm::Stmt {
    let short = || {
        mk_call(
            "operator_lt",
            vec![
                mk_call("data_lengthoflist", vec![mk_str(list)]),
                index.clone(),
            ],
        )
    };
    mk_block(
        "control_if",
        vec![short()],
        vec![mk_block(
            "control_repeat_until",
            vec![mk_call("operator_not", vec![short()])],
            vec![arena_push(list, mk_str(""))],
        )],
    )
}

fn arena_grow(list: &str, to: usize) -> rasm::Stmt {
    mk_block(
        "control_repeat_until",
        vec![mk_call(
            "operator_not",
            vec![mk_call(
                "operator_lt",
                vec![
                    mk_call("data_lengthoflist", vec![mk_str(list)]),
                    mk_num(to.to_string()),
                ],
            )],
        )],
        vec![arena_push(list, mk_str(""))],
    )
}

/// A boolean read out of storage, converted back into a **block**.
///
/// Scratch keeps a boolean either as its own `true`/`false` or — in a list, or in
/// a declaration loaded from `project.json` — as the text of one, and a boolean
/// *input* is hexagonal: it is wired to a block, never to a value. `<value =
/// "true">` is that block, and right for both shapes: `Cast.compare` falls back to
/// `String(value)` as soon as one side is not a number, and `String(true)` is
/// `"true"`. So one comparison reads a stored `true`, a stored `"true"` and a
/// stored `false` all correctly.
#[must_use]
fn read_bool(ty: Ty, expr: rasm::Expr) -> rasm::Expr {
    if ty == Ty::Bool {
        mk_call("operator_equals", vec![expr, mk_str("true")])
    } else {
        expr
    }
}

/// A boolean *literal* written in source: `true` or `false`.
///
/// Scratch has no `true`/`false` block, so there is nothing to lower a literal to
/// but a comparison — and that is what it is, a constant one. `1 = 1` is `true`
/// and `1 = 0` is `false`, in one block each.
#[must_use]
fn bool_literal(value: bool) -> rasm::Expr {
    let (left, right) = if value { ("1", "1") } else { ("1", "0") };
    mk_call("operator_equals", vec![mk_num(left), mk_num(right)])
}

/// The literal an expression is written as, when it is written as one.
fn literal_of(expr: &Expr) -> Option<ast::Literal> {
    let kind = match expr {
        Expr::Number { text, .. } => ast::LiteralKind::Number(text.clone()),
        Expr::Str { text, .. } => ast::LiteralKind::Str(text.clone()),
        Expr::Bool { value, .. } => ast::LiteralKind::Bool(*value),
        _ => return None,
    };
    Some(ast::Literal {
        kind,
        span: expr.span(),
    })
}

// ---------------------------------------------------------------------------
// The prelude
// ---------------------------------------------------------------------------

/// The prelude module, parsed once from the source that ships with the compiler.
#[derive(Debug)]
struct Prelude {
    macros: HashMap<String, Rc<MacroDef>>,
}

impl Prelude {
    fn load() -> Result<Self> {
        let source = Rc::new(Source::new("std/prelude.rav", PRELUDE));
        let file = crate::parser::parse(&source)?;
        let mut macros = HashMap::new();
        for item in &file.items {
            match item {
                Item::Macro(decl) => {
                    macros.insert(
                        decl.name.name.clone(),
                        Rc::new(MacroDef::from_decl(decl, Origin::Prelude)),
                    );
                }
                other => {
                    return Err(Error::msg(format!(
                        "the prelude may only contain macros, found a {}",
                        other.describe()
                    )))
                }
            }
        }
        Ok(Prelude { macros })
    }
}

// ---------------------------------------------------------------------------
// Macro definitions
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Origin {
    Prelude,
    File(PathBuf),
}

#[derive(Clone, Debug)]
struct MacroDef {
    name: String,
    params: Vec<ast::MacroParam>,
    result: ast::MacroResult,
    body: MacroBody,
    origin: Origin,
    defined_at: Span,
}

impl MacroDef {
    fn from_decl(decl: &ast::MacroDecl, origin: Origin) -> Self {
        Self {
            name: decl.name.name.clone(),
            params: decl.params.clone(),
            result: decl.result,
            body: decl.body.clone(),
            origin,
            defined_at: decl.name.span,
        }
    }

    fn from_fn(decl: &ast::FnDecl, path: &Path) -> Self {
        let params: Vec<ast::MacroParam> = decl
            .params
            .iter()
            .map(|param| ast::MacroParam {
                name: param.name.clone(),
                kind: ast::MacroParamKind::Expr(Some(param.ty)),
                span: param.span,
            })
            .collect();
        // `fn` parameters are written without `$`, so the body's references to
        // them are plain names: turn them into parameters before the expander
        // sees them, which is the whole of the sugar.
        let names: Vec<String> = decl.params.iter().map(|p| p.name.name.clone()).collect();
        let body = paramify(&decl.body, &names);
        Self {
            name: decl.name.name.clone(),
            params,
            result: ast::MacroResult::Expr(decl.ret),
            body: MacroBody::Expr(body),
            origin: Origin::File(path.to_path_buf()),
            defined_at: decl.name.span,
        }
    }

    /// Where this macro came from, for a diagnostic note.
    fn origin_note(&self) -> String {
        match &self.origin {
            Origin::Prelude => "it is defined in std::prelude".to_string(),
            Origin::File(path) => {
                format!("it is defined in {}:{}", short(path), self.defined_at.pos)
            }
        }
    }

    fn signature(&self) -> String {
        let params: Vec<String> = self
            .params
            .iter()
            .map(|p| format!("${}: {}", p.name.name, p.kind.describe()))
            .collect();
        let result = match self.result {
            ast::MacroResult::Expr(ty) => ty.name().to_string(),
            ast::MacroResult::Stmts => "stmts".to_string(),
        };
        format!("{}({}) -> {result}", self.name, params.join(", "))
    }
}

// ---------------------------------------------------------------------------
// Procedures
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct ProcInfo {
    name: String,
    params: Vec<(String, Scalar)>,
    /// `Some` when the procedure declares `-> ty` and returns a value.
    ret: Option<Ty>,
    warp: bool,
    body: Block,
    /// Where the `proc` name was written.
    defined_at: Span,
    source: Rc<Source>,
}

/// A block-scoped local: its cell in `_vms`, and the type it holds.
///
/// A struct-typed binding's cell is the **base** of its frame; the fields are
/// the cells after it, at offsets the layout decided.
#[derive(Clone, Copy, Debug)]
struct Local {
    addr: Addr,
    ty: Ty,
}

/// A field of a declared struct, with the cell offset the layout gave it.
#[derive(Clone, Debug)]
struct FieldInfo {
    name: String,
    ty: Ty,
    offset: usize,
}

/// A declared `struct`, and the frame it lays out.
#[derive(Clone, Debug)]
struct StructInfo {
    name: String,
    fields: Vec<FieldInfo>,
    /// How many cells one instance occupies.
    size: usize,
}

impl StructInfo {
    fn field(&self, name: &str) -> Option<&FieldInfo> {
        self.fields.iter().find(|f| f.name == name)
    }
}

/// Which list a cell lives in.
///
/// The split is by **lifetime**, and it is what makes the memory dynamic:
///
/// * [`Arena::Local`] is the target's arena, `_vms`. It holds what has to
///   outlive a script — a `var`, and a `proc`'s frame, which a re-entrant
///   (recursive) procedure shares with itself. It is grown on demand and never
///   shrinks, because its size is decided by the program, not by the data.
/// * [`Arena::Global`] is the project arena, `_gvm`, on the stage.
/// * [`Arena::Stack`] is the running script's own stack, `_stack<n>`. Every
///   block-scoped cell — a `let`, a `for` counter, a temporary — is *pushed*
///   when its declaration runs and *popped* when its block ends, so the list
///   grows and shrinks with the scopes that use it. Each script has its own, so
///   two scripts running at once cannot see each other's frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arena {
    Local,
    Global,
    Stack(usize),
}

impl Arena {
    /// The list this arena lives in.
    fn list(self) -> String {
        match self {
            Arena::Local => VMS.to_string(),
            Arena::Global => GLOBAL_VM.to_string(),
            Arena::Stack(index) => format!("{STACK}{index}"),
        }
    }
}

/// Where a place lives: a constant cell, and which arena it is in.
#[derive(Clone, Copy, Debug)]
struct Addr {
    cell: usize,
    arena: Arena,
}

impl Addr {
    fn read(self) -> rasm::Expr {
        arena_read(self.cell, &self.arena.list())
    }

    fn write(self, value: rasm::Expr) -> rasm::Stmt {
        arena_write(self.cell, &self.arena.list(), value)
    }

    /// The same arena, `offset` cells further on.
    fn offset(self, offset: usize) -> Self {
        Self {
            cell: self.cell + offset,
            arena: self.arena,
        }
    }
}

/// The `proc` whose body is currently being lowered.
#[derive(Clone, Debug)]
struct ProcContext {
    /// The procedure's name.
    name: String,
    /// Where it was declared, for a diagnostic that has to point somewhere the
    /// user wrote rather than inside the prelude.
    span: Span,
    /// The declared return type, if any.
    ret: Option<Ty>,
    /// The cell that holds the return value.
    cell: usize,
}

// ---------------------------------------------------------------------------
// One target
// ---------------------------------------------------------------------------

struct Unit<'a> {
    program: &'a Program,
    plan: &'a TargetPlan,
    globals: &'a Globals,

    macros: HashMap<String, Rc<MacroDef>>,
    consts: HashMap<String, ast::Literal>,
    /// The `struct`s this target can see, by interned id.
    structs: HashMap<crate::ty::StructId, Rc<StructInfo>>,
    /// Names `watch`ed in this target: each gets a visible Scratch mirror.
    watches: HashSet<String>,

    /// The target's own non-public variables, plus macro temporaries.
    locals: Vec<VarInfo>,
    procs: HashMap<String, Rc<ProcInfo>>,
    costumes: Vec<String>,
    sounds: Vec<String>,

    warnings: Vec<Diag>,
    hygiene: usize,
    in_macro: bool,
    pending: Vec<String>,
    emitted: HashSet<String>,

    /// The block scopes currently open, innermost last.
    scopes: Vec<HashMap<String, Local>>,
    /// The target's own `var` declarations, which are in scope everywhere in
    /// the target rather than in one block.
    base_scope: HashMap<String, Local>,
    /// How many `_vms` cells have been handed out. Cell 1 is the first.
    cells: usize,
    /// The value each cell starts with, by cell index - 1. A cell with no
    /// entry starts empty.
    cell_init: Vec<Option<rasm::Literal>>,
    /// Which script is being lowered. Scripts are numbered in source order, and
    /// each one has its own stack.
    script: usize,
    /// `true` while a script body is being lowered, so a block-scoped cell goes
    /// on that script's stack rather than into the target arena.
    in_script: bool,
    /// How many cells the running script's stack holds right now.
    depth: usize,
    /// The deepest cell each script's stack reaches, by script index. A script
    /// that never allocates a cell gets no stack list at all.
    stacks: Vec<usize>,
    /// Statements a returning call hoisted above the statement being lowered.
    pre: Vec<rasm::Stmt>,
    /// The return cell of each value-returning `proc` in this target.
    ret_cells: HashMap<String, usize>,
    /// How many expansions are open right now.
    macro_depth: usize,
    /// The macros whose definitions reach themselves, and a chain that says how.
    macro_cycles: HashMap<String, Vec<String>>,
    /// The `proc` body currently being lowered, if any.
    proc_context: Option<ProcContext>,
    /// The procedures that reach themselves, and a call chain that says how.
    recursion: HashMap<String, Vec<String>>,
}

impl<'a> Unit<'a> {
    fn new(
        program: &'a Program,
        plan: &'a TargetPlan,
        globals: &'a Globals,
        prelude: &'a Prelude,
    ) -> Result<Self> {
        let mut unit = Unit {
            program,
            plan,
            globals,
            macros: prelude.macros.clone(),
            consts: HashMap::new(),
            structs: HashMap::new(),
            watches: HashSet::new(),
            locals: Vec::new(),
            procs: HashMap::new(),
            costumes: Vec::new(),
            sounds: Vec::new(),
            warnings: Vec::new(),
            hygiene: 0,
            in_macro: false,
            pending: Vec::new(),
            emitted: HashSet::new(),
            scopes: Vec::new(),
            base_scope: HashMap::new(),
            cells: 0,
            cell_init: Vec::new(),
            script: 0,
            in_script: false,
            depth: 0,
            // Script numbers start at 1, so index 0 is never a stack.
            stacks: vec![0],
            pre: Vec::new(),
            ret_cells: HashMap::new(),
            macro_depth: 0,
            macro_cycles: HashMap::new(),
            proc_context: None,
            recursion: HashMap::new(),
        };
        unit.collect()?;
        unit.compute_recursion();
        unit.compute_macro_cycles();
        Ok(unit)
    }

    /// Register every name the target can see.
    fn collect(&mut self) -> Result<()> {
        let source = self.plan.main.source.clone();
        let items = items_of(&self.plan.main);
        let path = self.plan.main.path.clone();
        for item in &items {
            self.register(&path, &source, item)?;
        }
        for module in &self.plan.modules {
            let source = module.source.clone();
            let path = module.path.clone();
            for item in items_of(module) {
                if item.is_public() {
                    self.register(&path, &source, &item)?;
                }
            }
        }
        Ok(())
    }

    fn register(&mut self, path: &Path, source: &Rc<Source>, item: &Item) -> Result<()> {
        match item {
            Item::Var(var) => {
                if let Some(error) = reserved(&var.name.name, var.name.span, source) {
                    return Err(error);
                }
                if var.public {
                    // Project-wide; `Globals` owns it and every target sees it.
                    return Ok(());
                }
                if self.globals.var(&var.name.name).is_some() {
                    return Err(Error::new(
                        source
                            .error(
                                var.name.span.pos,
                                format!(
                                    "`{}` would shadow a project-wide variable of the same name",
                                    var.name.name
                                ),
                            )
                            .note("raven has no shadowing; rename one of the two"),
                    ));
                }
                if self.locals.iter().any(|v| v.name == var.name.name) {
                    return Err(Error::new(source.error(
                        var.name.span.pos,
                        format!("`{}` is declared twice", var.name.name),
                    )));
                }
                let mut info = var_info(var, false)?;
                if let Ty::Struct(id) = info.ty {
                    // A struct is a frame: `size` cells, the first of which is
                    // the binding's own cell.
                    let layout = self.struct_of(id, source, var.name.span)?;
                    let base = self.alloc_cells(layout.size);
                    info.cell = base;
                    self.init_struct(base, &layout, &var.init, source, var.name.span)?;
                    self.base_scope.insert(
                        var.name.name.clone(),
                        Local {
                            addr: Addr {
                                cell: base,
                                arena: Arena::Local,
                            },
                            ty: var.ty,
                        },
                    );
                } else if !info.is_list {
                    // A target-level scalar is a cell in the target's own
                    // arena. It is in scope for every script and every `proc`
                    // body of the target, so it goes in the base scope.
                    let cell = self.alloc_cell();
                    self.set_cell_init(cell, info.init.clone());
                    info.cell = cell;
                    self.base_scope.insert(
                        var.name.name.clone(),
                        Local {
                            addr: Addr {
                                cell,
                                arena: Arena::Local,
                            },
                            ty: var.ty,
                        },
                    );
                }
                self.locals.push(info);
            }
            Item::Const(decl) => {
                if let Some(error) = reserved(&decl.name.name, decl.name.span, source) {
                    return Err(error);
                }
                if self.consts.contains_key(&decl.name.name) {
                    return Err(duplicate(source, &decl.name, "constant"));
                }
                self.consts
                    .insert(decl.name.name.clone(), decl.value.clone());
            }
            Item::Proc(decl) => {
                if let Some(error) = reserved(&decl.name.name, decl.name.span, source) {
                    return Err(error);
                }
                if self.procs.contains_key(&decl.name.name) {
                    return Err(duplicate(source, &decl.name, "procedure"));
                }
                self.procs.insert(
                    decl.name.name.clone(),
                    Rc::new(ProcInfo {
                        name: decl.name.name.clone(),
                        params: decl
                            .params
                            .iter()
                            .map(|p| (p.name.name.clone(), p.ty))
                            .collect(),
                        ret: decl.ret,
                        warp: decl.warp,
                        body: decl.body.clone(),
                        defined_at: decl.name.span,
                        source: source.clone(),
                    }),
                );
            }
            Item::Fn(decl) => {
                if let Some(error) = reserved(&decl.name.name, decl.name.span, source) {
                    return Err(error);
                }
                self.check_macro_duplicate(source, &decl.name, "function")?;
                self.macros.insert(
                    decl.name.name.clone(),
                    Rc::new(MacroDef::from_fn(decl, path)),
                );
            }
            Item::Macro(decl) => {
                if let Some(error) = reserved(&decl.name.name, decl.name.span, source) {
                    return Err(error);
                }
                self.check_macro_duplicate(source, &decl.name, "macro")?;
                self.macros.insert(
                    decl.name.name.clone(),
                    Rc::new(MacroDef::from_decl(decl, Origin::File(path.to_path_buf()))),
                );
            }
            Item::Struct(decl) => {
                if let Some(error) = reserved(&decl.name.name, decl.name.span, source) {
                    return Err(error);
                }
                let info = self.layout_struct(decl, source)?;
                self.structs.insert(info.0, Rc::new(info.1));
            }
            Item::Watch(decl) => {
                for name in &decl.names {
                    if let Some(error) = reserved(&name.name, name.span, source) {
                        return Err(error);
                    }
                    self.watches.insert(name.name.clone());
                }
            }
            Item::Broadcast(_)
            | Item::Costume(_)
            | Item::Sound(_)
            | Item::Target(_)
            | Item::Script(_) => {}
        }
        Ok(())
    }

    /// A user definition may replace a prelude macro, but not another user's.
    ///
    /// The prelude is documented as shadowable; two definitions in the same file
    /// are still a mistake.
    fn check_macro_duplicate(&self, source: &Source, name: &Ident, what: &str) -> Result<()> {
        match self.macros.get(&name.name) {
            Some(existing) if matches!(existing.origin, Origin::Prelude) => Ok(()),
            Some(_) => Err(duplicate(source, name, what)),
            None => Ok(()),
        }
    }

    /// Record which procedures are recursive, and a chain through the cycle.
    ///
    /// A `_vms` cell belongs to a declaration site, not to a call, so a `let`
    /// inside a recursive `proc` would be one shared cell rather than storage per
    /// call. The graph is syntactic: a `proc` calls another when a call statement
    /// or a call expression names it.
    fn compute_recursion(&mut self) {
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        for (name, info) in &self.procs {
            let mut calls = Vec::new();
            collect_calls_block(&info.body, &mut calls);
            calls.retain(|called| self.procs.contains_key(called));
            calls.sort();
            calls.dedup();
            edges.insert(name.clone(), calls);
        }
        let mut recursion = HashMap::new();
        for name in self.procs.keys() {
            if let Some(chain) = cycle_through(&edges, name) {
                recursion.insert(name.clone(), chain);
            }
        }
        self.recursion = recursion;
    }

    /// Record which macros reach themselves through their own definitions.
    ///
    /// The graph is syntactic, and only the definition counts: a call written in
    /// a macro body is a step of that expansion, while the same name arriving
    /// through a substituted argument belongs to the caller. That is exactly the
    /// difference between `macro a() { a(); }` — which never terminates — and a
    /// `for` inside a `for`, which does.
    fn compute_macro_cycles(&mut self) {
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        for (name, def) in &self.macros {
            let mut calls = Vec::new();
            match &def.body {
                MacroBody::Stmts(stmts) => collect_calls_block(stmts, &mut calls),
                MacroBody::Expr(expr) => collect_calls_expr(expr, &mut calls),
            }
            calls.retain(|called| self.macros.contains_key(called));
            calls.sort();
            calls.dedup();
            edges.insert(name.clone(), calls);
        }
        let mut cycles = HashMap::new();
        for name in edges.keys() {
            if let Some(chain) = cycle_through(&edges, name) {
                cycles.insert(name.clone(), chain);
            }
        }
        self.macro_cycles = cycles;
    }

    // -- driver -----------------------------------------------------------

    fn run(&mut self) -> Result<rasm::File> {
        let source = self.plan.main.source.clone();
        let body = items_of(&self.plan.main);
        // A `watch` names state: it has to be something this target can see.
        for name in self.watches.clone() {
            if !self.locals.iter().any(|v| v.name == name) && self.globals.var(&name).is_none() {
                return Err(Error::new(
                    source
                        .error(
                            Span::default().pos,
                            format!("`{name}` is watched but never declared"),
                        )
                        .note(
                            "`watch` follows a `var` or a `list` that is declared in this target",
                        ),
                ));
            }
        }
        let mut out: Vec<rasm::Item> = Vec::new();

        // Costumes and sounds first: a script may mention either by name, and
        // the menu checker resolves names against what the target declares.
        for item in &body {
            match item {
                Item::Costume(decl) => {
                    self.costumes.push(decl.name.clone());
                    out.push(rasm::Item::Costume(rasm::CostumeDecl {
                        name: decl.name.clone(),
                        path: self.asset_path(&decl.path),
                        center: decl
                            .center
                            .as_ref()
                            .and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?))),
                        pos: decl.span.pos,
                        path_pos: decl.path_span.pos,
                    }));
                }
                Item::Sound(decl) => {
                    self.sounds.push(decl.name.clone());
                    out.push(rasm::Item::Sound(rasm::SoundDecl {
                        name: decl.name.clone(),
                        path: self.asset_path(&decl.path),
                        pos: decl.span.pos,
                        path_pos: decl.path_span.pos,
                    }));
                }
                _ => {}
            }
        }

        let mut scripts: Vec<rasm::Stmt> = Vec::new();
        for item in &body {
            if let Item::Script(script) = item {
                // Every script gets its own stack, so two of them running at
                // once cannot pull the ground out from under each other.
                self.script += 1;
                self.in_script = true;
                self.depth = 0;
                let stmt = self.script(script, &source)?;
                self.in_script = false;
                self.depth = 0;
                scripts.push(stmt);
            }
        }

        self.drain_procs(&mut out)?;

        // Now the arenas' sizes are known, so every script can start by growing
        // them. The calls come first, before the script's own stack is emptied
        // and before anything reaches for a cell.
        let mut prologue: Vec<rasm::Stmt> = Vec::new();
        if self.cells > 0 {
            prologue.push(mk_stmt(RESERVE_VMS, Vec::new()));
        }
        if self.globals.arena > 0 {
            prologue.push(mk_stmt(RESERVE_GVM, Vec::new()));
        }
        for mut stmt in scripts {
            let mut body = stmt.body.take().unwrap_or_default();
            let mut head = prologue.clone();
            head.append(&mut body);
            stmt.body = Some(head);
            out.push(rasm::Item::Stmt(stmt));
        }

        // Declarations first, so the emitted file reads top-down: the arenas,
        // then one stack per script that keeps block-scoped state.
        let mut locals: Vec<rasm::Item> = Vec::new();
        if self.cells > 0 {
            locals.push(self.vms_item());
        }
        for (index, high) in self.stacks.iter().enumerate() {
            if *high > 0 {
                locals.push(rasm::Item::List(rasm::ListDecl {
                    global: false,
                    visible: false,
                    monitor: rasm::MonitorSpec::default(),
                    name: format!("{STACK}{index}"),
                    init: Vec::new(),
                    pos: Pos::default(),
                }));
            }
        }
        locals.extend(self.locals.iter().filter_map(|var| {
            let mut item = var.as_list_item()?;
            // A `watch`ed list already has storage; it only has to be shown.
            if self.watches.contains(&var.name) {
                if let rasm::Item::List(list) = &mut item {
                    list.visible = true;
                }
            }
            Some(item)
        }));
        locals.append(&mut out);

        // A `watch`ed scalar is a real Scratch variable with a visible
        // monitor, which the cell writes keep in step; a `watch`ed list already
        // has a monitor, so it only has to be shown.
        let mut mirrored: Vec<&VarInfo> = self
            .locals
            .iter()
            .filter(|v| !v.is_list && self.watches.contains(&v.name))
            .collect();
        let global_mirrors: Vec<&VarInfo> = self
            .globals
            .vars
            .iter()
            .filter(|v| !v.is_list && self.watches.contains(&v.name))
            .collect();
        mirrored.extend(global_mirrors);
        for var in mirrored {
            locals.push(rasm::Item::Var(rasm::VarDecl {
                global: var.global,
                visible: true,
                monitor: rasm::MonitorSpec::default(),
                name: var.name.clone(),
                init: var.init.clone(),
                pos: var.pos,
            }));
        }

        // The memory manager, emitted only when there is an arena to grow.
        if self.cells > 0 {
            let inits: Vec<(usize, rasm::Literal)> = self
                .cell_init
                .iter()
                .enumerate()
                .filter_map(|(index, value)| Some((index + 1, value.clone()?)))
                .collect();
            locals.push(self.reserve_proc(RESERVE_VMS, VMS, &inits));
        }
        if self.globals.arena > 0 {
            // A Scratch custom block belongs to one target, so a sprite cannot
            // call the stage's copy: every target that touches `_gvm` grows its
            // own.
            let inits: Vec<(usize, rasm::Literal)> = self
                .globals
                .vars
                .iter()
                .filter(|v| !v.is_list)
                .map(|v| (v.cell, v.init.clone()))
                .collect();
            locals.push(self.reserve_proc(RESERVE_GVM, GLOBAL_VM, &inits));
        }
        out = locals;

        Ok(rasm::File {
            uses: Vec::new(),
            target: Some(rasm::TargetDecl {
                kind: if self.plan.kind == ast::TargetKind::Stage {
                    raven_asm::ast::TargetKind::Stage
                } else {
                    raven_asm::ast::TargetKind::Sprite
                },
                name: self.plan.name.clone(),
                pos: Pos::default(),
                items: out,
            }),
            items: Vec::new(),
        })
    }

    /// Add the project-wide declarations, which only the stage file carries.
    fn emit_globals(&self, file: &mut rasm::File) {
        if let Some(target) = &mut file.target {
            let mut globals: Vec<rasm::Item> = Vec::new();
            if self.globals.arena > 0 {
                globals.push(self.global_vms_item());
            }
            // The console is a list, so a log line is one `add`. It is
            // declared only when something logs, and its monitor starts
            // hidden: the developer ticks it in the editor when wanted.
            if self.globals.console {
                globals.push(rasm::Item::List(rasm::ListDecl {
                    global: true,
                    visible: false,
                    monitor: rasm::MonitorSpec::default(),
                    name: CONSOLE.to_string(),
                    init: Vec::new(),
                    pos: Pos::default(),
                }));
            }
            globals.extend(self.globals.vars.iter().filter_map(VarInfo::as_list_item));
            globals.extend(self.globals.broadcasts.iter().map(|name| {
                rasm::Item::Broadcast(rasm::BroadcastDecl {
                    name: name.clone(),
                    pos: Pos::default(),
                })
            }));
            globals.append(&mut target.items);
            target.items = globals;
        }
    }

    /// Emit every procedure reachable from the target's scripts, once.
    fn drain_procs(&mut self, out: &mut Vec<rasm::Item>) -> Result<()> {
        while let Some(name) = self.pending.pop() {
            if !self.emitted.insert(name.clone()) {
                continue;
            }
            let Some(info) = self.procs.get(&name).cloned() else {
                continue;
            };
            let params: Vec<rasm::Param> = info
                .params
                .iter()
                .map(|(param, kind)| rasm::Param {
                    name: param.clone(),
                    kind: match kind {
                        Scalar::Str => raven_asm::ast::ParamKind::String,
                        Scalar::Num => raven_asm::ast::ParamKind::Number,
                        Scalar::Bool => raven_asm::ast::ParamKind::Boolean,
                    },
                })
                .collect();
            let body = info.body.clone();
            // Only a value-returning `proc` owns a return cell.
            let cell = if info.ret.is_some() {
                self.ret_cell_for(&info.name)
            } else {
                0
            };
            let saved = self.proc_context.take();
            self.proc_context = Some(ProcContext {
                name: info.name.clone(),
                span: info.defined_at,
                ret: info.ret,
                cell,
            });
            let statements = self.block_stmts(&body, &info.source, &info.params);
            self.proc_context = saved;
            let statements = statements?;
            out.push(rasm::Item::Proc(rasm::ProcDecl {
                name: info.name.clone(),
                params,
                warp: info.warp,
                body: statements,
                pos: Pos::default(),
            }));
        }
        Ok(())
    }

    /// The target's `_vms` list, declared only when a cell was handed out.
    ///
    /// The list is declared with **one item per cell**, because Scratch
    /// cannot grow a list by replacing into it: `data_replaceitemoflist` runs
    /// `Cast.toListIndex(index, length, false)`, which rejects an index past the
    /// end, so a write into a shorter list is silently dropped. Pre-sizing the
    /// list is what makes a constant cell index work at all — and it is also why
    /// reads of an unwritten cell return `""` rather than failing.
    ///
    /// A cell that belongs to a `var` starts with that variable's declared
    /// value, because the declaration's initialiser is part of the arena: a
    /// `var score: num = 0;` is a cell that holds `0` before any script runs.
    /// That is how raven keeps a Scratch variable's "starts at its declared
    /// value" behaviour without declaring a Scratch variable.
    fn vms_item(&self) -> rasm::Item {
        // The arena starts **empty** and is grown by the memory manager the
        // first time a script needs it, so nothing is ever reserved that the
        // program does not use.
        self.arena_item(VMS, false, 0)
    }

    /// The project-wide arena, declared on the stage and visible everywhere.
    fn global_vms_item(&self) -> rasm::Item {
        // Empty for the same reason as `_vms`: it is grown on demand, and the
        // declared starting values are written by the memory manager once.
        rasm::Item::List(rasm::ListDecl {
            global: true,
            visible: false,
            monitor: rasm::MonitorSpec::default(),
            name: GLOBAL_VM.to_string(),
            init: Vec::new(),
            pos: Pos::default(),
        })
    }

    /// The `warp` procedure that grows an arena to its size and puts the
    /// declared starting values in place — the whole of the memory manager.
    #[allow(clippy::unused_self)]
    fn reserve_proc(&self, name: &str, list: &str, inits: &[(usize, rasm::Literal)]) -> rasm::Item {
        let mut body = vec![arena_grow(list, self.reserve_to(list))];
        for (cell, value) in inits {
            body.push(arena_write(*cell, list, literal_of_rasm(value)));
        }
        rasm::Item::Proc(rasm::ProcDecl {
            name: name.to_string(),
            params: Vec::new(),
            warp: true,
            body: vec![mk_block(
                "control_if",
                vec![mk_call(
                    "operator_lt",
                    vec![
                        mk_call("data_lengthoflist", vec![mk_str(list)]),
                        mk_num(self.reserve_to(list).to_string()),
                    ],
                )],
                body,
            )],
            pos: Pos::default(),
        })
    }

    /// How far an arena has to grow: past the last cell the program can use.
    fn reserve_to(&self, list: &str) -> usize {
        if list == GLOBAL_VM {
            self.globals.arena
        } else {
            self.cells
        }
    }

    /// One arena list of `cells` items, each carrying its starting value.
    fn arena_item(&self, name: &str, global: bool, cells: usize) -> rasm::Item {
        let init = (1..=cells)
            .map(|cell| {
                self.cell_init
                    .get(cell - 1)
                    .and_then(Clone::clone)
                    .unwrap_or_else(|| rasm::Literal::Str(String::new()))
            })
            .collect();
        rasm::Item::List(rasm::ListDecl {
            global,
            visible: false,
            monitor: rasm::MonitorSpec::default(),
            name: name.to_string(),
            init,
            pos: Pos::default(),
        })
    }

    /// Record the value a cell starts with.
    fn set_cell_init(&mut self, cell: usize, value: rasm::Literal) {
        while self.cell_init.len() < cell {
            self.cell_init.push(None);
        }
        self.cell_init[cell - 1] = Some(value);
    }

    /// Hand out the next cell index. Cells are 1-based, as Scratch lists are.
    fn alloc_cell(&mut self) -> usize {
        self.cells += 1;
        self.cells
    }

    /// Hand out `n` cells and return the first one, 1-based.
    fn alloc_cells(&mut self, n: usize) -> usize {
        let base = self.cells + 1;
        self.cells += n;
        base
    }

    // -- the running script's stack ---------------------------------------

    /// Push one cell onto the running script's stack.
    fn alloc_stack_cell(&mut self) -> usize {
        self.depth += 1;
        self.note_stack();
        self.depth
    }

    /// Push `n` cells onto the running script's stack.
    fn alloc_stack_cells(&mut self, n: usize) -> usize {
        let base = self.depth + 1;
        self.depth += n;
        self.note_stack();
        base
    }

    /// A cell for the body being lowered.
    ///
    /// In a script that is a *push* on the script's stack, and the block that
    /// declared it pops it again. In a `proc` it is a cell of the target's
    /// arena, because a procedure's frame is shared by every call of it — which
    /// is what makes recursion work with a constant index.
    fn alloc_body_cell(&mut self) -> usize {
        if self.in_script {
            self.alloc_stack_cell()
        } else {
            self.alloc_cell()
        }
    }

    fn alloc_body_cells(&mut self, n: usize) -> usize {
        if self.in_script {
            self.alloc_stack_cells(n)
        } else {
            self.alloc_cells(n)
        }
    }

    /// The arena a body's block-scoped cells live in.
    fn body_arena(&self) -> Arena {
        if self.in_script {
            Arena::Stack(self.script)
        } else {
            Arena::Local
        }
    }

    fn note_stack(&mut self) {
        while self.stacks.len() <= self.script {
            self.stacks.push(0);
        }
        self.stacks[self.script] = self.stacks[self.script].max(self.depth);
    }

    /// A fresh temporary cell, already holding `value`.
    ///
    /// On a stack that is a push; in the arena it is a write. Either way it is
    /// one block, and the caller gets the address to read and rewrite.
    fn temp_cell(&mut self, value: rasm::Expr) -> (Addr, rasm::Stmt) {
        let arena = self.body_arena();
        let addr = Addr {
            cell: self.alloc_body_cell(),
            arena,
        };
        let init = match arena {
            Arena::Stack(_) => arena_push(&arena.list(), value),
            _ => addr.write(value),
        };
        (addr, init)
    }

    /// The statement that gives a freshly allocated body cell its value.
    fn init_cell(&self, addr: Addr, value: rasm::Expr) -> rasm::Stmt {
        match addr.arena {
            Arena::Stack(_) => arena_push(&addr.arena.list(), value),
            _ => addr.write(value),
        }
    }

    // -- structs ----------------------------------------------------------

    /// Lay a `struct` out: one cell per scalar field, in declaration order.
    ///
    /// A field of struct type is a nested frame, so `size` is the sum of the
    /// fields' sizes and a field's offset is a constant the compiler decides.
    fn layout_struct(
        &self,
        decl: &ast::StructDecl,
        source: &Source,
    ) -> Result<(crate::ty::StructId, StructInfo)> {
        let id = crate::ty::intern_struct(&decl.name.name);
        let mut offset = 0;
        let mut fields: Vec<FieldInfo> = Vec::new();
        for field in &decl.fields {
            if fields.iter().any(|f| f.name == field.name.name) {
                return Err(duplicate(source, &field.name, "field"));
            }
            let size = self.frame_size(field.ty, source, field.name.span)?;
            if size > MAX_STRUCT_SIZE.saturating_sub(offset) {
                return Err(Error::new(
                    source
                        .error(
                            field.name.span.pos,
                            format!("`{}` makes this struct too large", decl.name.name),
                        )
                        .span(field.name.span.len.max(1))
                        .note(format!(
                            "a struct may occupy at most {MAX_STRUCT_SIZE} cells"
                        )),
                ));
            }
            fields.push(FieldInfo {
                name: field.name.name.clone(),
                ty: field.ty,
                offset,
            });
            offset += size;
        }
        Ok((
            id,
            StructInfo {
                name: decl.name.name.clone(),
                fields,
                size: offset,
            },
        ))
    }

    /// The layout of a declared struct.
    fn struct_of(
        &self,
        id: crate::ty::StructId,
        source: &Source,
        span: Span,
    ) -> Result<Rc<StructInfo>> {
        self.structs.get(&id).cloned().ok_or_else(|| {
            Error::new(
                source
                    .error(
                        span.pos,
                        format!("`{}` is not a declared struct", crate::ty::struct_name(id)),
                    )
                    .span(span.len.max(1))
                    .note("declare it with `struct Name { field: num, … }`"),
            )
        })
    }

    /// How many cells a value of `ty` occupies inside a frame.
    fn frame_size(&self, ty: Ty, source: &Source, span: Span) -> Result<usize> {
        match ty {
            Ty::Num | Ty::Str | Ty::Bool => Ok(1),
            Ty::Struct(id) => Ok(self.struct_of(id, source, span)?.size),
            Ty::List(_) | Ty::Map(..) => Err(Error::new(
                source
                    .error(
                        span.pos,
                        format!("`{}` cannot be a struct field", ty.name()),
                    )
                    .span(span.len.max(1))
                    .note("a struct is a run of one cell per scalar field")
                    .note("a list is a Scratch list; declare it beside the struct"),
            )),
        }
    }

    /// Write a struct declaration's starting values into its frame.
    ///
    /// A declaration starts life from literals, exactly as a `var` does, so the
    /// values can live in the arena list itself and cost no script.
    fn init_struct(
        &mut self,
        base: usize,
        layout: &StructInfo,
        init: &ast::Initializer,
        source: &Source,
        span: Span,
    ) -> Result<()> {
        let ast::Initializer::Fields(fields) = init else {
            return Err(Error::new(
                source
                    .error(
                        span.pos,
                        format!(
                            "`{}` is a struct, so it is built as `{} {{ field: value, … }}`",
                            layout.name, layout.name
                        ),
                    )
                    .span(span.len.max(1)),
            ));
        };
        for field in &layout.fields {
            let Some((_, value)) = fields.iter().find(|(name, _)| name.name == field.name) else {
                return Err(Error::new(
                    source
                        .error(
                            span.pos,
                            format!("`{}` is missing its `{}` field", layout.name, field.name),
                        )
                        .span(span.len.max(1))
                        .note("every field of a struct starts with a value"),
                ));
            };
            self.init_field(base + field.offset, field, value, source, span)?;
        }
        for (name, _) in fields {
            if layout.field(&name.name).is_none() {
                return Err(Error::new(
                    source
                        .error(
                            name.span.pos,
                            format!("`{}` has no field `{}`", layout.name, name.name),
                        )
                        .span(name.span.len.max(1)),
                ));
            }
        }
        Ok(())
    }

    fn init_field(
        &mut self,
        cell: usize,
        field: &FieldInfo,
        value: &Expr,
        source: &Source,
        span: Span,
    ) -> Result<()> {
        let Ty::Struct(id) = field.ty else {
            let Some(literal) = literal_of(value) else {
                return Err(Error::new(
                    source
                        .error(value.span().pos, "a declaration starts from a literal")
                        .span(value.span().len.max(1))
                        .note("a computed starting value belongs in a script"),
                ));
            };
            if literal.ty() != field.ty {
                return Err(Error::new(
                    source
                        .error(
                            value.span().pos,
                            format!(
                                "`{}` is `{}`, but its value is `{}`",
                                field.name,
                                field.ty.name(),
                                literal.ty().name()
                            ),
                        )
                        .span(value.span().len.max(1)),
                ));
            }
            self.set_cell_init(cell, to_literal(&literal));
            return Ok(());
        };
        let layout = self.struct_of(id, source, span)?;
        let Expr::Struct { fields, .. } = value else {
            return Err(Error::new(
                source
                    .error(
                        value.span().pos,
                        format!("`{}` needs a struct literal", field.name),
                    )
                    .span(value.span().len.max(1)),
            ));
        };
        for sub in &layout.fields {
            let Some((_, sub_value)) = fields.iter().find(|(name, _)| name.name == sub.name) else {
                return Err(Error::new(
                    source
                        .error(
                            value.span().pos,
                            format!("`{}` is missing its `{}` field", layout.name, sub.name),
                        )
                        .span(value.span().len.max(1)),
                ));
            };
            self.init_field(cell + sub.offset, sub, sub_value, source, span)?;
        }
        Ok(())
    }

    /// Write a struct *expression* into a fresh frame, as statements.
    ///
    /// This is what a `let p: Point = Point { … };` costs: one
    /// `data_replaceitemoflist` per field, and nothing else.
    fn build_struct(
        &mut self,
        layout: &StructInfo,
        fields: &[(Ident, Expr)],
        source: &Rc<Source>,
        params: &[(String, Scalar)],
        span: Span,
    ) -> Result<(usize, Vec<rasm::Stmt>)> {
        let arena = self.body_arena();
        let base = self.alloc_body_cells(layout.size);
        let mut out = Vec::new();
        if let Arena::Stack(_) = arena {
            // A stack frame is pushed, then written field by field.
            for _ in 0..layout.size {
                out.push(arena_push(&arena.list(), mk_str("")));
            }
        }
        for field in &layout.fields {
            let Some((_, value)) = fields.iter().find(|(name, _)| name.name == field.name) else {
                return Err(Error::new(
                    source
                        .error(
                            span.pos,
                            format!("`{}` is missing its `{}` field", layout.name, field.name),
                        )
                        .span(span.len.max(1)),
                ));
            };
            if let Ty::Struct(id) = field.ty {
                let nested = self.struct_of(id, source, span)?;
                let Expr::Struct {
                    fields: nested_fields,
                    ..
                } = value
                else {
                    return Err(Error::new(
                        source
                            .error(
                                value.span().pos,
                                format!("`{}` needs a struct literal", field.name),
                            )
                            .span(value.span().len.max(1)),
                    ));
                };
                let nested_arena = self.body_arena();
                let (nested_base, nested_stmts) =
                    self.build_struct(&nested, nested_fields, source, params, value.span())?;
                out.extend(nested_stmts);
                // Move the nested frame into this one, field by field.
                for sub in &nested.fields {
                    out.push(
                        Addr {
                            cell: base + field.offset + sub.offset,
                            arena,
                        }
                        .write(
                            Addr {
                                cell: nested_base + sub.offset,
                                arena: nested_arena,
                            }
                            .read(),
                        ),
                    );
                }
                continue;
            }
            let value = self.expr(value, source, params)?;
            expect(
                &value,
                field.ty,
                &format!("the `{}` field", field.name),
                source,
            )?;
            out.push(
                Addr {
                    cell: base + field.offset,
                    arena,
                }
                .write(value.expr),
            );
        }
        for (name, _) in fields {
            if layout.field(&name.name).is_none() {
                return Err(Error::new(
                    source
                        .error(
                            name.span.pos,
                            format!("`{}` has no field `{}`", layout.name, name.name),
                        )
                        .span(name.span.len.max(1)),
                ));
            }
        }
        let saved = std::mem::take(&mut self.pre);
        self.pre = saved;
        Ok((base, out))
    }
    /// The cell that holds `proc`'s return value, allocated on first use.
    fn ret_cell_for(&mut self, name: &str) -> usize {
        if let Some(cell) = self.ret_cells.get(name) {
            return *cell;
        }
        let cell = self.alloc_cell();
        self.ret_cells.insert(name.to_string(), cell);
        cell
    }

    fn lookup_scope(&self, name: &str) -> Option<Local> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
            .or_else(|| self.base_scope.get(name).copied())
    }

    fn bind(&mut self, name: &str, addr: Addr, ty: Ty) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), Local { addr, ty });
        }
    }

    /// A statement list that is a block: its `let`s die with the block.
    ///
    /// On a script's stack that is literal: the cells the block pushed are
    /// popped again when it ends, so the list grows and shrinks with the scopes
    /// that use it. Inside a `proc` the cells are arena cells, which outlive the
    /// block on purpose — a procedure's frame is one that every call shares.
    fn block_stmts(
        &mut self,
        stmts: &[Stmt],
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Vec<rasm::Stmt>> {
        self.scopes.push(HashMap::new());
        let depth = self.depth;
        let mut result = self.stmts(stmts, source, params);
        if self.in_script && self.depth > depth {
            if let Ok(body) = &mut result {
                let list = Arena::Stack(self.script).list();
                // Pop from the top down, which is the order they were pushed in.
                for cell in ((depth + 1)..=self.depth).rev() {
                    body.push(arena_pop(cell, &list));
                }
            }
        }
        self.depth = depth;
        self.scopes.pop();
        result
    }

    /// A `$name` that survived expansion has no value to stand for.
    fn reject_unsubstituted(&self, name: &str, span: Span, source: &Source) -> Result<()> {
        if name.starts_with('$') {
            return Err(Error::new(
                source
                    .error(
                        span.pos,
                        format!("`{name}` is a macro parameter with no value"),
                    )
                    .span(span.len.max(1))
                    .note(
                        "a `$name` is only meaningful where the macro that declares it supplies it",
                    ),
            ));
        }
        Ok(())
    }

    /// A name the target declares is a value, not a call — even when the
    /// standard library has a block of the same name.
    fn reject_value_call(&self, name: &str, span: Span, source: &Source) -> Result<()> {
        let is_value = self.lookup_scope(name).is_some()
            || self.consts.contains_key(name)
            || self.locals.iter().any(|v| v.name == name)
            || self.globals.var(name).is_some();
        if is_value {
            return Err(Error::new(
                source
                    .error(span.pos, format!("`{name}` is a value, not a call"))
                    .span(span.len.max(1))
                    .note("the target's own `let`, `var` and `const` come before a bare standard-library name; rename one of the two"),
            ));
        }
        Ok(())
    }

    /// A `let` inside a recursive `proc` has one cell, not one per call.
    ///
    /// A `_vms` cell belongs to a declaration site, so every level of the
    /// recursion would share it. Refuse rather than emit a project that reads
    /// the wrong value.
    fn reject_recursive_let(&self, span: Span, source: &Source) -> Result<()> {
        let Some(context) = &self.proc_context else {
            return Ok(());
        };
        let Some(chain) = self.recursion.get(&context.name) else {
            return Ok(());
        };
        // A `let` the macro expander wrote is not in the caller's file, so point
        // at the `proc` the user did write.
        let at = if self.in_macro { context.span } else { span };
        Err(Error::new(
            source
                .error(
                    at.pos,
                    format!(
                        "`{}` is recursive, so a `let` in its body has no storage of its own",
                        context.name
                    ),
                )
                .span(at.len.max(1))
                .note(format!("the cycle is {}", chain.join(" → ")))
                .note("a `_vms` cell is one cell shared by every call, not a stack frame")
                .note("pass the value as a parameter, keep it in a `var`, or write the recursion without block-local storage"),
        ))
    }

    /// Asset paths are written relative to the emitted raven-asm project, which
    /// lives in `<output>/asm`, so `--emit-asm` produces a buildable tree.
    fn asset_path(&self, path: &str) -> String {
        if Path::new(path).is_absolute() {
            return path.to_string();
        }
        let root = &self.program.root;
        let raw = self
            .program
            .manifest
            .project
            .output
            .as_deref()
            .unwrap_or(crate::identity::DEFAULT_OUTPUT_DIR);
        let joined = root.join(raw);
        let output = if raw.ends_with(".sb3") {
            joined
                .parent()
                .map_or_else(|| root.clone(), Path::to_path_buf)
        } else {
            joined
        };
        relative(&output.join("asm"), &root.join(path))
    }

    // -- scripts ----------------------------------------------------------

    fn script(&mut self, script: &ast::ScriptDecl, source: &Rc<Source>) -> Result<rasm::Stmt> {
        let opcode = match stdlib::BINDINGS
            .iter()
            .find(|row| matches!(row.binding, Binding::Hat(name) if name == script.hat.name.name))
        {
            Some(row) => row.opcode,
            None => {
                let hats: Vec<String> = stdlib::BINDINGS
                    .iter()
                    .filter_map(|row| match row.binding {
                        Binding::Hat(name) => Some(format!("`{name}`")),
                        _ => None,
                    })
                    .collect();
                return Err(Error::new(
                    source
                        .error(
                            script.hat.name.span.pos,
                            format!("there is no `{}` hat", script.hat.name.name),
                        )
                        .note(format!("the hats are {}", hats.join(", "))),
                ));
            }
        };
        let spec = catalog::block(opcode).expect("bound hats are catalog blocks");
        let args = self.arguments(spec, &script.hat.args, source, &[])?;
        let mut hoisted = std::mem::take(&mut self.pre);
        let body = self.block_stmts(&script.body, source, &[])?;
        // The script's prologue: its stack is emptied, and the arenas are grown
        // to what this target can use and given their declared values. Both are
        // idempotent, so a script that runs a second time pays one block.
        if self.stacks.get(self.script).copied().unwrap_or(0) > 0 {
            // A stack list exists only for a script that pushes something onto
            // it, and only that script has to empty it.
            hoisted.push(arena_clear(&Arena::Stack(self.script).list()));
        }
        // The calls that grow the arenas are added once the whole target is
        // lowered: how big an arena is is not known until then.
        hoisted.extend(body);
        Ok(mk_block(opcode, args, hoisted))
    }

    // -- statements -------------------------------------------------------

    fn stmts(
        &mut self,
        stmts: &[Stmt],
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Vec<rasm::Stmt>> {
        let mut out = Vec::new();
        for (index, stmt) in stmts.iter().enumerate() {
            // `return` is a `control_stop` on the custom block, and a
            // `control_stop` has no bottom notch: the block after it cannot be
            // attached to anything. raven-asm refuses that, so raven says so
            // first, where the user can see which statement it means.
            if matches!(stmt, Stmt::Return(_)) {
                if let Some(next) = stmts.get(index + 1) {
                    return Err(Error::new(
                        source
                            .error(
                                next.span().pos,
                                "this statement is unreachable: `return` already stopped the `proc`",
                            )
                            .span(next.span().len.max(1))
                            .note("`return` is a `control_stop` block, and nothing attaches below one")
                            .note("move this statement above the `return`, or delete it"),
                    ));
                }
            }
            out.extend(self.stmt(stmt, source, params)?);
        }
        Ok(out)
    }

    /// Lower one statement, with any returning call it hoisted above it.
    ///
    /// A `proc` call in expression position lowers to statements, so while an
    /// expression is lowered those statements collect in [`Unit::pre`]; this
    /// wrapper splices them in front of the statement that needed them, which is
    /// what keeps evaluation order left to right.
    fn stmt(
        &mut self,
        stmt: &Stmt,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Vec<rasm::Stmt>> {
        let saved = std::mem::take(&mut self.pre);
        let inner = self.stmt_inner(stmt, source, params)?;
        let hoisted = std::mem::take(&mut self.pre);
        let mut out = saved;
        out.extend(hoisted);
        out.extend(inner);
        Ok(out)
    }

    fn stmt_inner(
        &mut self,
        stmt: &Stmt,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Vec<rasm::Stmt>> {
        match stmt {
            Stmt::Let(decl) => {
                self.reject_unsubstituted(&decl.name.name, decl.name.span, source)?;
                if let Some(error) = reserved(&decl.name.name, decl.name.span, source) {
                    return Err(error);
                }
                self.reject_recursive_let(decl.name.span, source)?;
                // `let p: Point = Point { … };` makes a frame rather than
                // writing one cell: a struct is a place, not a value.
                if let Expr::Struct { name, fields, span } = &decl.value {
                    let id = crate::ty::intern_struct(&name.name);
                    let layout = self.struct_of(id, source, name.span)?;
                    if let Some(want) = decl.ty {
                        if want != Ty::Struct(id) {
                            return Err(Error::new(
                                source
                                    .error(
                                        decl.name.span.pos,
                                        format!(
                                            "`{}` is declared `{}`, but the value is a `{}`",
                                            decl.name.name,
                                            want.name(),
                                            layout.name
                                        ),
                                    )
                                    .span(decl.name.span.len.max(1)),
                            ));
                        }
                    }
                    let (base, writes) =
                        self.build_struct(&layout, fields, source, params, *span)?;
                    self.bind(
                        &decl.name.name,
                        Addr {
                            cell: base,
                            arena: self.body_arena(),
                        },
                        Ty::Struct(id),
                    );
                    return Ok(writes);
                }
                let value = self.expr(&decl.value, source, params)?;
                let ty = match decl.ty {
                    Some(want) => {
                        expect(&value, want, &format!("`{}`", decl.name.name), source)?;
                        want
                    }
                    None => value.ty,
                };
                if !ty.is_scalar() {
                    return Err(Error::new(
                        source
                            .error(
                                decl.name.span.pos,
                                format!(
                                    "`{}` is a `{}`, which is not a value",
                                    decl.name.name,
                                    ty.name()
                                ),
                            )
                            .span(decl.name.span.len.max(1))
                            .note("a struct is a place; declare it with `var`, or build it here"),
                    ));
                }
                let addr = Addr {
                    cell: self.alloc_body_cell(),
                    arena: self.body_arena(),
                };
                self.bind(&decl.name.name, addr, ty);
                Ok(vec![self.init_cell(addr, value.expr)])
            }
            Stmt::Assign(assign) => {
                let value = self.expr(&assign.value, source, params)?;
                self.assign_place(&assign.target, value, source, params)
            }
            Stmt::CompoundAssign(assign) => {
                let value = self.expr(&assign.value, source, params)?;
                expect(
                    &value,
                    Ty::Num,
                    &format!("the right of `{}`", compound_spelling(assign.op)),
                    source,
                )?;
                let opcode = assign
                    .op
                    .opcode()
                    .expect("a compound assignment is one of `+ - * / %`");
                self.compound_place(&assign.target, opcode, value.expr, source, params)
            }
            Stmt::Method(stmt) => self.method_stmt(stmt, source, params),
            Stmt::Return(ret) => {
                let Some(context) = self.proc_context.clone() else {
                    return Err(Error::new(
                        source
                            .error(
                                ret.span.pos,
                                "`return` is only allowed inside a `proc` body",
                            )
                            .span(ret.span.len.max(1))
                            .note("a script runs to its end; there is nothing to return from"),
                    ));
                };
                let Some(expr) = &ret.value else {
                    return Ok(vec![mk_stmt("control_stop", vec![mk_str("this script")])]);
                };
                let Some(want) = context.ret else {
                    return Err(Error::new(
                        source
                            .error(ret.span.pos, "this `proc` does not return a value")
                            .span(ret.span.len.max(1))
                            .note("declare it `-> ty`, or write a bare `return;`"),
                    ));
                };
                let value = self.expr(expr, source, params)?;
                expect(&value, want, "the returned value", source)?;
                Ok(vec![
                    cell_write(context.cell, value.expr),
                    mk_stmt("control_stop", vec![mk_str("this script")]),
                ])
            }
            Stmt::Call(call) => self.call_statement(call, source, params),
            Stmt::If(if_stmt) => {
                let cond = self.expr(&if_stmt.cond, source, params)?;
                expect(&cond, Ty::Bool, "an `if` condition", source)?;
                let mut head = std::mem::take(&mut self.pre);
                let then_branch = self.block_stmts(&if_stmt.then_branch, source, params)?;
                match &if_stmt.else_branch {
                    Some(else_branch) => {
                        let else_body = self.block_stmts(else_branch, source, params)?;
                        head.push(mk_block_else(
                            "control_if_else",
                            vec![cond.expr],
                            then_branch,
                            else_body,
                        ));
                    }
                    None => head.push(mk_block("control_if", vec![cond.expr], then_branch)),
                }
                Ok(head)
            }
            Stmt::Loop(loop_stmt) => {
                // A condition that needed statements of its own — a
                // value-returning `proc` call, a map `get` — is evaluated by
                // running them. They must run again for every test, or the loop
                // would test the first answer forever, so the hoisted statements
                // are repeated at the end of the body.
                let mut retest: Vec<rasm::Stmt> = Vec::new();
                let (opcode, args, mut head) = match &loop_stmt.kind {
                    ast::LoopKind::Repeat(times) => {
                        let times = self.expr(times, source, params)?;
                        expect(&times, Ty::Num, "a `repeat` count", source)?;
                        (
                            "control_repeat",
                            vec![times.expr],
                            std::mem::take(&mut self.pre),
                        )
                    }
                    ast::LoopKind::RepeatUntil(cond) => {
                        let cond = self.expr(cond, source, params)?;
                        expect(&cond, Ty::Bool, "a `repeat_until` condition", source)?;
                        let head = std::mem::take(&mut self.pre);
                        if !head.is_empty() {
                            retest = head.clone();
                        }
                        ("control_repeat_until", vec![cond.expr], head)
                    }
                    ast::LoopKind::Forever => {
                        ("control_forever", Vec::new(), std::mem::take(&mut self.pre))
                    }
                };
                let mut body = self.block_stmts(&loop_stmt.body, source, params)?;
                body.extend(retest);
                head.push(mk_block(opcode, args, body));
                Ok(head)
            }
            Stmt::Match(match_stmt) => self.match_statement(match_stmt, source, params),
            Stmt::Macro(call) => {
                let expansion = self.expand(call, source, params)?;
                match expansion {
                    Expansion::Stmts(stmts) => self.lower_macro_stmts(&stmts, source, params),
                    Expansion::Expr(_) => Err(Error::new(
                        source
                            .error(
                                call.span.pos,
                                format!("`{}` produces a value, not a statement", call.name.name),
                            )
                            .note("use it where a value is expected"),
                    )),
                }
            }
            Stmt::Param(ident) => Err(Error::new(source.error(
                ident.span.pos,
                format!("`${}` is only meaningful inside a macro", ident.name),
            ))),
            Stmt::Var(decl) => {
                self.reject_unsubstituted(&decl.name.name, decl.name.span, source)?;
                if let Some(error) = reserved(&decl.name.name, decl.name.span, source) {
                    return Err(error);
                }
                if !self.in_macro {
                    return Err(Error::new(
                        source
                            .error(
                                decl.name.span.pos,
                                format!("`{}` is declared inside a body", decl.name.name),
                            )
                            .note("Scratch variables belong to a target: declare it beside the other `var`s")
                            .note("a `var` statement is only allowed inside a macro, where it becomes a hygienic temporary")
                            .note(format!("a block-scoped value is a `let {} = …;`", decl.name.name)),
                    ));
                }
                // Declaring a variable that already exists is not an error here:
                // it is how a macro asserts that its counter is available.
                if self.locals.iter().any(|v| v.name == decl.name.name)
                    || self.globals.var(&decl.name.name).is_some()
                {
                    return Ok(Vec::new());
                }
                let info = var_info(decl, false)?;
                if info.is_list {
                    self.locals.push(info);
                    return Ok(Vec::new());
                }
                // A macro's scalar temporary is one cell, exactly like a `let`,
                // and the declaration writes its starting value into it.
                let ast::Initializer::Value(literal) = &decl.init else {
                    return Err(Error::new(source.error(
                        decl.name.span.pos,
                        format!("`{}` needs one starting value", decl.name.name),
                    )));
                };
                if literal.ty() != decl.ty {
                    return Err(Error::new(
                        source
                            .error(
                                decl.name.span.pos,
                                format!(
                                    "`{}` is declared `{}`, but its value is `{}`",
                                    decl.name.name,
                                    decl.ty.name(),
                                    literal.ty().name()
                                ),
                            )
                            .span(decl.name.span.len),
                    ));
                }
                let addr = Addr {
                    cell: self.alloc_body_cell(),
                    arena: self.body_arena(),
                };
                self.bind(&decl.name.name, addr, decl.ty);
                Ok(vec![self.init_cell(addr, literal_expr(literal))])
            }
        }
    }

    /// Lower a `match`.
    ///
    /// Cost: one `control_if_else` per pattern arm, the first of which carries
    /// the fallback body as its `else`. When there is more than one arm and the
    /// subject is not duplicable — a variable or a list read, not a literal or
    /// arithmetic over literals — the subject is first copied into one fresh
    /// `_vms` cell, so it is read once and every arm compares against the copy.
    /// That is one extra `data_replaceitemoflist` block. A pure subject, or a
    /// single-arm `match`, needs neither the cell nor the block.
    fn match_statement(
        &mut self,
        match_stmt: &ast::MatchStmt,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Vec<rasm::Stmt>> {
        // A wildcard is the fallback, so nothing may follow it.
        if let Some(position) = match_stmt.arms.iter().position(|arm| arm.pattern.is_none()) {
            if position + 1 != match_stmt.arms.len() {
                let arm = &match_stmt.arms[position];
                return Err(Error::new(
                    source
                        .error(arm.span.pos, "the `_` arm must be last")
                        .span(arm.span.len.max(1))
                        .note("`_` matches anything, so an arm after it can never run"),
                ));
            }
        }
        // Raising a returning call in the subject happens before this point.
        let subject = self.expr(&match_stmt.subject, source, params)?;
        let mut head = std::mem::take(&mut self.pre);
        let needs_cell =
            match_stmt.arms.len() > 1 && !self.purity_of(&match_stmt.subject).may_be_duplicated();
        let compare = if needs_cell {
            let (addr, init) = self.temp_cell(subject.expr);
            head.push(init);
            addr.read()
        } else {
            subject.expr
        };
        // The last wildcard arm, if any, becomes the final `else`.
        let mut arms: Vec<&ast::MatchArm> = match_stmt.arms.iter().collect();
        let fallback = if arms.last().is_some_and(|arm| arm.pattern.is_none()) {
            Some(arms.pop().expect("checked"))
        } else {
            None
        };
        let mut tail: Vec<rasm::Stmt> = match &fallback {
            Some(arm) => self.block_stmts(&arm.body, source, params)?,
            None => Vec::new(),
        };
        for arm in arms.into_iter().rev() {
            let Some(pattern) = arm.pattern.as_ref() else {
                continue;
            };
            let value = self.pattern_value(pattern, source)?;
            let pattern_ty = value.ty();
            if pattern_ty != subject.ty
                && !(matches!(pattern_ty, Ty::Num | Ty::Str)
                    && matches!(subject.ty, Ty::Num | Ty::Str))
            {
                return Err(Error::new(
                    source
                        .error(
                            pattern.span().pos,
                            format!(
                                "this arm matches {}, but the subject is `{}`",
                                value.describe(),
                                subject.ty.name()
                            ),
                        )
                        .span(pattern.span().len),
                ));
            }
            let body = self.block_stmts(&arm.body, source, params)?;
            let test = mk_call(
                "operator_equals",
                vec![compare.clone(), literal_expr(&value)],
            );
            tail = vec![mk_block_else("control_if_else", vec![test], body, tail)];
        }
        head.extend(tail);
        Ok(head)
    }

    /// The literal a `match` pattern stands for, resolving a `const` name.
    fn pattern_value(&self, pattern: &ast::Pattern, source: &Source) -> Result<ast::Literal> {
        match pattern {
            ast::Pattern::Literal(literal) => Ok(literal.clone()),
            ast::Pattern::Name(path) => {
                let name = path.last().name.clone();
                if path.is_single() {
                    if let Some(literal) = self.consts.get(&name) {
                        return Ok(literal.clone());
                    }
                }
                Err(Error::new(
                    source
                        .error(
                            path.span.pos,
                            format!("there is no constant `{}`", path.display()),
                        )
                        .span(path.span.len.max(1))
                        .note("a `match` pattern is a literal or a `const`"),
                ))
            }
        }
    }

    fn call_statement(
        &mut self,
        call: &ast::CallStmt,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Vec<rasm::Stmt>> {
        let single = call.callee.is_single();

        // A macro or `fn` in statement position. A macro that takes a `block`
        // parameter is written with the block after the call, exactly as the
        // body-taking blocks are.
        if single && self.macros.contains_key(&call.callee.last().name) {
            let mut args: Vec<MacroArg> = call.args.iter().cloned().map(MacroArg::Expr).collect();
            if let Some(body) = &call.body {
                args.push(MacroArg::Block(body.clone()));
            }
            let macro_call = ast::MacroCall {
                name: call.callee.last().clone(),
                args,
                span: call.span,
            };
            let expansion = self.expand(&macro_call, source, params)?;
            return match expansion {
                Expansion::Stmts(stmts) => self.lower_macro_stmts(&stmts, source, params),
                Expansion::Expr(_) => Err(Error::new(
                    source
                        .error(
                            call.span.pos,
                            format!("`{}` produces a value", call.callee.last().name),
                        )
                        .note("use it where a value is expected"),
                )),
            };
        }

        // A procedure call.
        if single {
            let name = call.callee.last().name.clone();
            self.reject_unsubstituted(&name, call.callee.span, source)?;
            if let Some(info) = self.procs.get(&name).cloned() {
                if call.body.is_some() {
                    return Err(Error::new(source.error(
                        call.body_span.pos,
                        format!("`{name}` is a procedure and takes no `{{ … }}` body"),
                    )));
                }
                return self.proc_call_value(&name, &info, &call.args, call.span, source, params);
            }
            self.reject_value_call(&name, call.callee.span, source)?;
        }

        if let Some((module, name)) = self.split_std(&call.callee) {
            if module == CONSOLE_MODULE {
                return self.console_call(&name, &call.args, call.span, source, params);
            }
        }

        // A standard-library or body-taking block, qualified or by its unique
        // bare name.
        let Some((module, name)) = self.resolve_std(&call.callee, source)? else {
            return Err(Error::new(
                source
                    .error(
                        call.callee.span.pos,
                        format!("cannot find `{}`", call.callee.display()),
                    )
                    .span(call.callee.span.len)
                    .note("a statement is a procedure call, a standard-library call, or an assignment"),
            ));
        };
        let Some(opcode) = stdlib::opcode_for(&module, &name) else {
            return Err(Error::new(
                source
                    .error(
                        call.callee.span.pos,
                        format!("there is no `{module}::{name}`"),
                    )
                    .span(call.callee.span.len),
            ));
        };
        let spec = catalog::block(opcode).expect("bound opcodes are catalog blocks");
        let binding = stdlib::binding(opcode).expect("bound opcodes have a binding");
        if let Some(why) = binding.forbidden() {
            return Err(forbidden_block(source, &call.callee, &module, &name, why));
        }
        let args = self.arguments(spec, &call.args, source, params)?;
        if binding.takes_body() {
            let Some(body) = &call.body else {
                return Err(Error::new(
                    source
                        .error(
                            call.span.pos,
                            format!("`{module}::{name}` takes a `{{ … }}` body"),
                        )
                        .note(format!("write `{module}::{name}(…) {{ … }}`")),
                ));
            };
            let mut head = std::mem::take(&mut self.pre);
            let body = self.block_stmts(body, source, params)?;
            head.push(mk_block(opcode, args, body));
            return Ok(head);
        }
        if let Some(body) = &call.body {
            let _ = body;
            return Err(Error::new(source.error(
                call.body_span.pos,
                format!("`{module}::{name}` takes no body"),
            )));
        }
        if binding.result().is_some() {
            return Err(Error::new(
                source
                    .error(
                        call.span.pos,
                        format!("`{module}::{name}` produces a value"),
                    )
                    .note("use it where a value is expected, not as a statement"),
            ));
        }
        Ok(vec![mk_stmt(opcode, args)])
    }

    /// A `proc` call, lowered to one custom-block call statement.
    /// `console::log(text)`, `console::clear()` and `console::count()`.
    ///
    /// The console is one Scratch list, `_console`, and each line is one item —
    /// so a log costs exactly one `add`, and the developer decides when to show
    /// the monitor. `log` takes any scalar and writes it as it would be printed,
    /// which is what `f"{x}"` does.
    fn console_call(
        &mut self,
        name: &str,
        args: &[Expr],
        span: Span,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Vec<rasm::Stmt>> {
        const WHAT: &str =
            "the console is `console::log(x)`, `console::clear()` and `console::count()`";
        match (name, args.len()) {
            ("log", 1) => {
                let value = self.expr(&args[0], source, params)?;
                Ok(vec![arena_push(CONSOLE, value.expr)])
            }
            ("clear", 0) => Ok(vec![arena_clear(CONSOLE)]),
            ("count", 0) => Err(Error::new(
                source
                    .error(span.pos, "`console::count` is not a statement")
                    .note("use it where a value is expected"),
            )),
            _ => Err(Error::new(
                source
                    .error(span.pos, format!("there is no `console::{name}` here"))
                    .span(span.len.max(1))
                    .note(WHAT),
            )),
        }
    }

    fn proc_call_value(
        &mut self,
        name: &str,
        info: &ProcInfo,
        call_args: &[Expr],
        span: Span,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Vec<rasm::Stmt>> {
        let expected: Vec<(String, Ty)> = info
            .params
            .iter()
            .map(|(name, kind)| (name.clone(), Ty::from(*kind)))
            .collect();
        if call_args.len() != expected.len() {
            return Err(arity_error(name, &expected, call_args, span, source));
        }
        let mut args = Vec::new();
        for (index, arg) in call_args.iter().enumerate() {
            let value = self.expr(arg, source, params)?;
            expect(
                &value,
                expected[index].1,
                &format!("`{}`", expected[index].0),
                source,
            )?;
            args.push(value.expr);
        }
        if !self.emitted.contains(name) {
            self.pending.push(name.to_string());
        }
        Ok(vec![mk_stmt(name, args)])
    }

    // -- expressions ------------------------------------------------------

    fn expr(
        &mut self,
        expr: &Expr,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Typed> {
        match expr {
            Expr::Number { text, span } => Ok(Typed {
                expr: mk_num(text.clone()),
                ty: Ty::Num,
                span: *span,
            }),
            Expr::Str { text, span } => Ok(Typed {
                expr: mk_str(text.clone()),
                ty: Ty::Str,
                span: *span,
            }),
            Expr::Bool { value, span } => Ok(Typed {
                expr: bool_literal(*value),
                ty: Ty::Bool,
                span: *span,
            }),
            Expr::Name(path) => self.name(path, source, params),
            Expr::Call(call) => self.call_expr(call, source, params),
            Expr::Index { list, index, span } => {
                let Expr::Name(path) = list.as_ref() else {
                    return Err(Error::new(
                        source
                            .error(span.pos, "only a declared list can be indexed")
                            .span(span.len),
                    ));
                };
                let name = path.last().name.clone();
                let ty = self.declared_ty(&name);
                if matches!(ty, Some(Ty::Map(..))) {
                    return Err(Error::new(
                        source
                            .error(span.pos, format!("`{name}` is a map"))
                            .span(span.len.max(1))
                            .note("a map is read with `name.get(key)`"),
                    ));
                }
                let element = self.list_element(&name, source, path.span)?;
                let index = self.expr(index, source, params)?;
                expect(&index, Ty::Num, "a list index", source)?;
                Ok(Typed {
                    expr: read_bool(
                        Ty::from(element),
                        mk_call("data_itemoflist", vec![index.expr, mk_str(name)]),
                    ),
                    ty: Ty::from(element),
                    span: *span,
                })
            }
            Expr::Field { name, span, .. } => {
                let (addr, ty) = self.place(expr, source)?;
                if !ty.is_scalar() {
                    return Err(no_value_for(source, *span, &name.name, ty));
                }
                Ok(Typed {
                    expr: read_bool(ty, addr.read()),
                    ty,
                    span: *span,
                })
            }
            Expr::Method {
                receiver,
                name,
                args,
                span: _,
            } => self.method_expr(expr, receiver, name, args, source, params),
            Expr::Struct { name, span, .. } => Err(Error::new(
                source
                    .error(
                        span.pos,
                        format!(
                            "`{}` is a struct, which is a place and not a value",
                            name.name
                        ),
                    )
                    .span(span.len.max(1))
                    .note(
                        "build it in a `let` or `var` declaration: `let p: Point = Point { … };`",
                    ),
            )),
            Expr::Unary { op, operand, span } => {
                let operand = self.expr(operand, source, params)?;
                match op {
                    ast::UnOp::Not => {
                        expect(&operand, Ty::Bool, "`!`", source)?;
                        Ok(Typed {
                            expr: mk_call("operator_not", vec![operand.expr]),
                            ty: Ty::Bool,
                            span: *span,
                        })
                    }
                    ast::UnOp::Neg => {
                        expect(&operand, Ty::Num, "unary `-`", source)?;
                        Ok(Typed {
                            expr: mk_call("operator_subtract", vec![mk_num("0"), operand.expr]),
                            ty: Ty::Num,
                            span: *span,
                        })
                    }
                }
            }
            Expr::Binary { op, lhs, rhs, span } => {
                self.binary(*op, lhs, rhs, *span, source, params)
            }
            Expr::Interpolated { parts, span } => self.interpolated(parts, *span, source, params),
            Expr::Macro(call) => {
                let def = self.macros.get(&call.name.name).cloned();
                let expansion = self.expand(call, source, params)?;
                match expansion {
                    Expansion::Expr(expr) => {
                        let typed = self.lower_macro_expr(&expr, source, params)?;
                        if let Some(def) = &def {
                            self.check_result(def, &typed, call.span, source)?;
                        }
                        Ok(typed)
                    }
                    Expansion::Stmts(_) => Err(Error::new(
                        source
                            .error(
                                call.span.pos,
                                format!("`{}` produces statements, not a value", call.name.name),
                            )
                            .note("use it as a statement"),
                    )),
                }
            }
            Expr::Param(ident) => Err(Error::new(source.error(
                ident.span.pos,
                format!("`${}` is only meaningful inside a macro", ident.name),
            ))),
        }
    }

    fn name(
        &mut self,
        path: &ast::Path,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Typed> {
        let span = path.span;
        if !path.is_single() {
            return Err(Error::new(
                source
                    .error(span.pos, format!("`{}` is not a value", path.display()))
                    .span(span.len)
                    .note("a menu value such as `Key::Space` is only valid where the block expects that menu"),
            ));
        }
        let name = path.last().name.clone();
        self.reject_unsubstituted(&name, span, source)?;
        if let Some(binding) = self.lookup_scope(&name) {
            if binding.ty.is_list() {
                return Err(Error::new(
                    source
                        .error(span.pos, format!("`{name}` is a list"))
                        .span(span.len)
                        .note("a list is not a value; read an item with `name[i]` or pass it to a `data::` block"),
                ));
            }
            if binding.ty.is_place() {
                return Err(no_value_for(source, span, &name, binding.ty));
            }
            return Ok(Typed {
                expr: read_bool(binding.ty, binding.addr.read()),
                ty: binding.ty,
                span,
            });
        }
        if let Some(literal) = self.consts.get(&name).cloned() {
            return Ok(Typed {
                expr: literal_expr(&literal),
                ty: literal.ty(),
                span,
            });
        }
        if let Some((_, kind)) = params.iter().find(|(param, _)| *param == name) {
            return Ok(Typed {
                expr: mk_call(kind.argument_reporter(), vec![mk_str(name.clone())]),
                ty: Ty::from(*kind),
                span,
            });
        }
        let local = self
            .locals
            .iter()
            .find(|v| v.name == name)
            .map(|v| (v.ty, v.is_list));
        let global = self.globals.var(&name).map(|v| (v.ty, v.is_list));
        let Some((ty, is_list)) = local.or(global) else {
            return Err(self.unknown_name(&name, span, source));
        };
        if is_list {
            return Err(Error::new(
                source
                    .error(span.pos, format!("`{name}` is a list"))
                    .span(span.len)
                    .note("a list is not a value; read an item with `name[i]` or pass it to a `data::` block"),
            ));
        }
        let expr = match self.globals.scalar_cell(&name) {
            Some(cell) => read_bool(ty, gcell_read(cell)),
            // A target-level `var` is in the base scope, so a scalar that is not
            // project-wide is always found by `lookup_scope` above.
            None => match self.locals.iter().find(|v| v.name == name) {
                Some(var) => read_bool(ty, cell_read(var.cell)),
                None => return Err(self.unknown_name(&name, span, source)),
            },
        };
        Ok(Typed { expr, ty, span })
    }

    fn unknown_name(&self, name: &str, span: Span, source: &Source) -> Error {
        let mut error = Error::new(
            source
                .error(span.pos, format!("cannot find `{name}`"))
                .span(span.len),
        );
        let candidates: Vec<&str> = self
            .locals
            .iter()
            .map(|v| v.name.as_str())
            .chain(self.globals.vars.iter().map(|v| v.name.as_str()))
            .chain(self.consts.keys().map(String::as_str))
            .collect();
        if let Some(best) = closest(name, &candidates) {
            error = error.note(format!("did you mean `{best}`?"));
        }
        error.note("a name must be declared before it is used")
    }

    /// `num(x)` and `str(x)`: a conversion between `num` and `str` that emits no
    /// block, because the target is untyped either way. Anything else — a
    /// `bool`, a list — has no single meaning and is refused.
    fn convert(
        &mut self,
        to: &str,
        call: &ast::CallExpr,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Typed> {
        let want = if to == "num" { Ty::Num } else { Ty::Str };
        let [arg] = call.args.as_slice() else {
            return Err(Error::new(
                source
                    .error(
                        call.span.pos,
                        format!(
                            "`{to}(…)` takes one argument, but {} were given",
                            call.args.len()
                        ),
                    )
                    .note("it retypes one value; it does not parse or format"),
            ));
        };
        let value = self.expr(arg, source, params)?;
        match value.ty {
            Ty::Num | Ty::Str => Ok(Typed {
                expr: value.expr,
                ty: want,
                span: call.span,
            }),
            found => Err(Error::new(
                source
                    .error(
                        call.span.pos,
                        format!("`{to}(…)` cannot convert `{}`", found.name()),
                    )
                    .span(call.span.len.max(1))
                    .note("the two free conversions are `num(x)` and `str(x)`, between `num` and `str`")
                    .note("there is no `bool(x)`: write the comparison you meant"),
            )),
        }
    }

    fn call_expr(
        &mut self,
        call: &ast::CallExpr,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Typed> {
        if call.callee.is_single() {
            let name = call.callee.last().name.clone();
            // `num(x)` and `str(x)`, the two free conversions. They are spelled
            // with a type keyword, so this cannot collide with a user item.
            if name == "num" || name == "str" {
                return self.convert(&name, call, source, params);
            }
            self.reject_unsubstituted(&name, call.callee.span, source)?;
            if let Some(def) = self.macros.get(&name).cloned() {
                let macro_call = ast::MacroCall {
                    name: call.callee.last().clone(),
                    args: call.args.iter().cloned().map(MacroArg::Expr).collect(),
                    span: call.span,
                };
                return match self.expand(&macro_call, source, params)? {
                    Expansion::Expr(expr) => {
                        let typed = self.lower_macro_expr(&expr, source, params)?;
                        self.check_result(&def, &typed, call.span, source)?;
                        Ok(typed)
                    }
                    Expansion::Stmts(_) => Err(Error::new(
                        source.error(call.span.pos, format!("`{}` produces statements", def.name)),
                    )),
                };
            }
            if let Some(info) = self.procs.get(&name).cloned() {
                let Some(ret) = info.ret else {
                    return Err(Error::new(
                        source
                            .error(call.span.pos, format!("`{name}` is a procedure"))
                            .span(call.callee.span.len)
                            .note("a `proc` runs at run time and cannot produce a value")
                            .note("if this is a computation, write an `fn`")
                            .note(format!(
                                "if it returns a value, declare it `-> ty`, as in `proc {name}(…) -> num`"
                            )),
                    ));
                };
                // The call runs first; its return value is copied into a fresh
                // cell so a second call to the same `proc` cannot overwrite it.
                let stmts =
                    self.proc_call_value(&name, &info, &call.args, call.span, source, params)?;
                let ret_cell = self.ret_cell_for(&name);
                let (temp, init) = self.temp_cell(cell_read(ret_cell));
                let mut stmts = stmts;
                stmts.push(init);
                self.pre.extend(stmts);
                return Ok(Typed {
                    expr: read_bool(ret, temp.read()),
                    ty: ret,
                    span: call.span,
                });
            }
            self.reject_value_call(&name, call.callee.span, source)?;
        }

        if let Some((module, name)) = self.split_std(&call.callee) {
            if module == CONSOLE_MODULE {
                return match (name.as_str(), call.args.len()) {
                    ("count", 0) => Ok(Typed {
                        expr: mk_call("data_lengthoflist", vec![mk_str(CONSOLE)]),
                        ty: Ty::Num,
                        span: call.span,
                    }),
                    ("log" | "clear", _) => Err(Error::new(
                        source
                            .error(
                                call.span.pos,
                                format!("`console::{name}` produces no value"),
                            )
                            .note("use it as a statement"),
                    )),
                    _ => Err(Error::new(
                        source
                            .error(call.span.pos, format!("there is no `console::{name}`"))
                            .note("the console is `console::log(x)`, `console::clear()` and `console::count()`"),
                    )),
                };
            }
        }

        if let Some((module, name)) = self.split_std(&call.callee) {
            if module == CONSOLE_MODULE {
                return match (name.as_str(), call.args.len()) {
                    ("count", 0) => Ok(Typed {
                        expr: mk_call("data_lengthoflist", vec![mk_str(CONSOLE)]),
                        ty: Ty::Num,
                        span: call.span,
                    }),
                    ("log" | "clear", _) => Err(Error::new(
                        source
                            .error(
                                call.span.pos,
                                format!("`console::{name}` produces no value"),
                            )
                            .note("use it as a statement"),
                    )),
                    _ => Err(Error::new(
                        source
                            .error(call.span.pos, format!("there is no `console::{name}`"))
                            .note("the console is `console::log(x)`, `console::clear()` and `console::count()`"),
                    )),
                };
            }
        }

        let Some((module, name)) = self.resolve_std(&call.callee, source)? else {
            return Err(self.unknown_name(&call.callee.last().name, call.callee.span, source));
        };
        let Some(opcode) = stdlib::opcode_for(&module, &name) else {
            return Err(Error::new(
                source
                    .error(
                        call.callee.span.pos,
                        format!("there is no `{module}::{name}`"),
                    )
                    .span(call.callee.span.len),
            ));
        };
        let spec = catalog::block(opcode).expect("bound opcodes are catalog blocks");
        let binding = stdlib::binding(opcode).expect("bound opcodes have a binding");
        if let Some(why) = binding.forbidden() {
            return Err(forbidden_block(source, &call.callee, &module, &name, why));
        }
        if binding.takes_body() {
            return Err(Error::new(source.error(
                call.span.pos,
                format!("`{module}::{name}` takes a `{{ … }}` body and no value"),
            )));
        }
        if binding.result().is_none() {
            return Err(Error::new(
                source
                    .error(
                        call.span.pos,
                        format!("`{module}::{name}` produces no value"),
                    )
                    .note("use it as a statement"),
            ));
        }
        let args = self.arguments(spec, &call.args, source, params)?;
        let ty = match binding.result().expect("value blocks produce a value") {
            Value::Num => Ty::Num,
            Value::Str => Ty::Str,
            Value::Bool => Ty::Bool,
            Value::OfVariable => {
                let first = call.args.first().ok_or_else(|| {
                    Error::new(source.error(call.span.pos, "this block needs a variable"))
                })?;
                let name = self.path_name(first, "a variable", source)?;
                self.variable_ty(&name, source, first.span())?
            }
            Value::ListElement => {
                let list = call.args.get(1).ok_or_else(|| {
                    Error::new(source.error(call.span.pos, "this block needs a list"))
                })?;
                let name = self.path_name(list, "a list", source)?;
                Ty::from(self.list_element(&name, source, list.span())?)
            }
        };
        Ok(Typed {
            expr: mk_call(opcode, args),
            ty,
            span: call.span,
        })
    }

    /// Check an expansion in expression position against the macro's declared
    /// result type.
    fn check_result(
        &self,
        def: &MacroDef,
        typed: &Typed,
        span: Span,
        source: &Source,
    ) -> Result<()> {
        let ast::MacroResult::Expr(want) = def.result else {
            return Ok(());
        };
        if typed.ty == want {
            return Ok(());
        }
        Err(Error::new(
            source
                .error(
                    span.pos,
                    format!(
                        "`{}` is declared `-> {}`, but it produces `{}`",
                        def.name,
                        want.name(),
                        typed.ty.name()
                    ),
                )
                .span(span.len.max(1))
                .note(format!("it is declared as {}", def.signature()))
                .note(def.origin_note()),
        ))
    }

    /// Operators need the scope to type their operands, exactly as a call does,
    /// which is why this takes the same context as the rest of the traversal.
    #[allow(clippy::too_many_arguments)]
    fn binary(
        &mut self,
        op: ast::BinOp,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Typed> {
        let lhs = self.expr(lhs, source, params)?;
        let rhs = self.expr(rhs, source, params)?;
        let join = |opcode: &str, ty: Ty| Typed {
            expr: mk_call(opcode, vec![lhs.expr.clone(), rhs.expr.clone()]),
            ty,
            span,
        };
        let both_num = |source: &Source| {
            expect(
                &lhs,
                Ty::Num,
                &format!("the left of `{}`", op.text()),
                source,
            )?;
            expect(
                &rhs,
                Ty::Num,
                &format!("the right of `{}`", op.text()),
                source,
            )
        };
        match op {
            ast::BinOp::Add => {
                both_num(source)?;
                Ok(join("operator_add", Ty::Num))
            }
            ast::BinOp::Sub => {
                both_num(source)?;
                Ok(join("operator_subtract", Ty::Num))
            }
            ast::BinOp::Mul => {
                both_num(source)?;
                Ok(join("operator_multiply", Ty::Num))
            }
            ast::BinOp::Div => {
                both_num(source)?;
                Ok(join("operator_divide", Ty::Num))
            }
            ast::BinOp::Rem => {
                both_num(source)?;
                Ok(join("operator_mod", Ty::Num))
            }
            ast::BinOp::Lt => {
                both_num(source)?;
                Ok(join("operator_lt", Ty::Bool))
            }
            ast::BinOp::Gt => {
                both_num(source)?;
                Ok(join("operator_gt", Ty::Bool))
            }
            ast::BinOp::Le => {
                both_num(source)?;
                Ok(Typed {
                    expr: mk_call(
                        "operator_not",
                        vec![mk_call("operator_gt", vec![lhs.expr, rhs.expr])],
                    ),
                    ty: Ty::Bool,
                    span,
                })
            }
            ast::BinOp::Ge => {
                both_num(source)?;
                Ok(Typed {
                    expr: mk_call(
                        "operator_not",
                        vec![mk_call("operator_lt", vec![lhs.expr, rhs.expr])],
                    ),
                    ty: Ty::Bool,
                    span,
                })
            }
            ast::BinOp::Eq | ast::BinOp::Ne => {
                let comparable = matches!(lhs.ty, Ty::Num | Ty::Str)
                    && matches!(rhs.ty, Ty::Num | Ty::Str)
                    || lhs.ty == rhs.ty;
                if !comparable {
                    return Err(Error::new(
                        source
                            .error(
                                span.pos,
                                format!(
                                    "`{}` cannot compare `{}` with `{}`",
                                    op.text(),
                                    lhs.ty.name(),
                                    rhs.ty.name()
                                ),
                            )
                            .span(span.len),
                    ));
                }
                let equals = mk_call("operator_equals", vec![lhs.expr, rhs.expr]);
                Ok(Typed {
                    expr: if op == ast::BinOp::Eq {
                        equals
                    } else {
                        mk_call("operator_not", vec![equals])
                    },
                    ty: Ty::Bool,
                    span,
                })
            }
            ast::BinOp::And | ast::BinOp::Or => {
                expect(
                    &lhs,
                    Ty::Bool,
                    &format!("the left of `{}`", op.text()),
                    source,
                )?;
                expect(
                    &rhs,
                    Ty::Bool,
                    &format!("the right of `{}`", op.text()),
                    source,
                )?;
                Ok(join(
                    if op == ast::BinOp::And {
                        "operator_and"
                    } else {
                        "operator_or"
                    },
                    Ty::Bool,
                ))
            }
        }
    }

    fn interpolated(
        &mut self,
        parts: &[ast::InterpPart],
        span: Span,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Typed> {
        let mut pieces: Vec<rasm::Expr> = Vec::new();
        for part in parts {
            match part {
                ast::InterpPart::Text(text) => {
                    if !text.is_empty() {
                        pieces.push(mk_str(text.clone()));
                    }
                }
                ast::InterpPart::Hole(expr) => {
                    // Interpolation is a text context, so any scalar fits.
                    let value = self.expr(expr, source, params)?;
                    pieces.push(value.expr);
                }
            }
        }
        let mut iter = pieces.into_iter();
        let Some(mut result) = iter.next() else {
            return Ok(Typed {
                expr: mk_str(""),
                ty: Ty::Str,
                span,
            });
        };
        for piece in iter {
            result = mk_call("operator_join", vec![result, piece]);
        }
        Ok(Typed {
            expr: result,
            ty: Ty::Str,
            span,
        })
    }

    // -- arguments by shape -----------------------------------------------

    /// Lower a block's arguments against the catalog's own description of them.
    fn arguments(
        &mut self,
        spec: &BlockSpec,
        args: &[Expr],
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Vec<rasm::Expr>> {
        if args.len() != spec.args.len() {
            return Err(Error::new(
                source
                    .error(
                        args.first().map_or(spec_block_span(spec), Expr::span).pos,
                        format!(
                            "`{}` takes {} argument{}, but {} were given",
                            spec.opcode,
                            spec.args.len(),
                            if spec.args.len() == 1 { "" } else { "s" },
                            args.len()
                        ),
                    )
                    .note(signature_of(spec)),
            ));
        }
        let mut out = Vec::new();
        for (arg, expr) in spec.args.iter().zip(args.iter()) {
            let lowered = match (arg.wire, arg.shape) {
                (Wire::Field, Shape::Variable) => {
                    let name = self.path_name(expr, "a variable", source)?;
                    let ty = self.variable_ty(&name, source, expr.span())?;
                    // A block that does arithmetic on the variable it names has
                    // to be told the variable holds a number.
                    if let Some(want) = variable_constraint(spec.opcode) {
                        if ty != want {
                            return Err(Error::new(
                                source
                                    .error(
                                        expr.span().pos,
                                        format!(
                                            "`{}` is `{}`, but `{}` needs `{}`",
                                            name,
                                            ty.name(),
                                            spec.opcode,
                                            want.name()
                                        ),
                                    )
                                    .span(expr.span().len)
                                    .note(format!(
                                        "Scratch stores a number here; a `{}` variable cannot be changed by a number",
                                        ty.name()
                                    )),
                            ));
                        }
                    }
                    mk_str(name)
                }
                (Wire::Field, Shape::List) => {
                    let name = self.path_name(expr, "a list", source)?;
                    self.list_element(&name, source, expr.span())?;
                    mk_str(name)
                }
                (Wire::Field, Shape::Broadcast) | (Wire::Input, Shape::Broadcast) => {
                    let Expr::Str { text, span } = expr else {
                        return Err(Error::new(
                            source
                                .error(expr.span().pos, "expected the name of a broadcast")
                                .span(expr.span().len)
                                .note("declare it with `broadcast \"name\";`"),
                        ));
                    };
                    if !self.globals.broadcasts.contains(text) {
                        let mut error =
                            Error::new(source.error(
                                span.pos,
                                format!("there is no broadcast called \"{text}\""),
                            ));
                        if !self.globals.broadcasts.is_empty() {
                            error = error.note(format!(
                                "the broadcasts are {}",
                                self.globals
                                    .broadcasts
                                    .iter()
                                    .map(|b| format!("\"{b}\""))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                        }
                        return Err(error);
                    }
                    mk_str(text.clone())
                }
                (_, Shape::Menu(id)) => self.menu_argument(id, expr, source, params)?,
                (_, Shape::ParamName) => {
                    return Err(Error::new(
                        source.error(expr.span().pos, "this block reads a procedure parameter"),
                    ))
                }
                (Wire::Field, _) => {
                    return Err(Error::new(source.error(
                        expr.span().pos,
                        format!("`{}` takes a fixed value here", spec.opcode),
                    )))
                }
                (Wire::Input, Shape::Bool) => {
                    let value = self.expr(expr, source, params)?;
                    expect(&value, Ty::Bool, "a boolean input", source)?;
                    value.expr
                }
                (Wire::Input, Shape::Text) => {
                    let value = self.expr(expr, source, params)?;
                    if !matches!(value.ty, Ty::Num | Ty::Str) {
                        return Err(Error::new(source.error(
                            value.span.pos,
                            format!(
                                "a text input takes `num` or `str`, found `{}`",
                                value.ty.name()
                            ),
                        )));
                    }
                    value.expr
                }
                (
                    Wire::Input,
                    Shape::Number | Shape::Whole | Shape::Integer | Shape::Angle | Shape::Positive,
                ) => {
                    let value = self.expr(expr, source, params)?;
                    expect(&value, Ty::Num, "a numeric input", source)?;
                    value.expr
                }
                (Wire::Input, Shape::Color) => {
                    let value = self.expr(expr, source, params)?;
                    match &value.expr {
                        rasm::Expr::Str(text, _) if text.starts_with('#') => value.expr,
                        _ => {
                            return Err(Error::new(source.error(
                                value.span.pos,
                                "a colour is written as a `\"#rrggbb\"` string",
                            )))
                        }
                    }
                }
                (Wire::Input, Shape::Variable) | (Wire::Input, Shape::List) => {
                    let name = self.path_name(expr, "a name", source)?;
                    mk_str(name)
                }
            };
            out.push(lowered);
        }
        Ok(out)
    }

    fn menu_argument(
        &mut self,
        menu_id: &str,
        expr: &Expr,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<rasm::Expr> {
        let type_name = menu::type_name(menu_id);
        if let Expr::Name(path) = expr {
            if path.segments.len() >= 2 {
                let prefix = &path.segments[path.segments.len() - 2].name;
                if *prefix != type_name {
                    return Err(Error::new(
                        source
                            .error(
                                path.span.pos,
                                format!("this input takes a `{type_name}`, not a `{prefix}`"),
                            )
                            .span(path.span.len),
                    ));
                }
                let variant = &path.last().name;
                let value = self.variant(menu_id, variant, path.last().span, source)?;
                return Ok(mk_str(value));
            }
        }
        // An open menu takes a literal or, in an input slot, any expression.
        let domain = menu::domain(menu_id);
        if !domain.is_enumerable() {
            let value = self.expr(expr, source, params)?;
            if matches!(value.ty, Ty::Num | Ty::Str) {
                return Ok(value.expr);
            }
        }
        Err(Error::new(
            source
                .error(
                    expr.span().pos,
                    format!("this input takes a `{type_name}` value"),
                )
                .span(expr.span().len)
                .note(format!("the values are {}", self.variant_list(menu_id))),
        ))
    }

    /// The Scratch string a variant names.
    fn variant(&self, menu_id: &str, variant: &str, span: Span, source: &Source) -> Result<String> {
        let domain = menu::domain(menu_id);
        let names: Vec<&str> = match domain {
            menu::Domain::Fixed(values) => values.to_vec(),
            menu::Domain::Sprites(extras) => extras.to_vec(),
            menu::Domain::Costumes => self.costumes.iter().map(String::as_str).collect(),
            menu::Domain::Backdrops => self
                .globals
                .stage_costumes
                .iter()
                .map(String::as_str)
                .collect(),
            menu::Domain::Sounds => self.sounds.iter().map(String::as_str).collect(),
            menu::Domain::Open => Vec::new(),
        };
        for value in names {
            if menu::variant(value) == variant {
                return Ok(value.to_string());
            }
        }
        // The special targets: `_mouse_`, `_edge_`, `_random_`, `_myself_`,
        // `_stage_`, under the names the menus give them.
        if let Some(value) = menu::special_value(variant) {
            let allowed = match domain {
                menu::Domain::Sprites(extras) => extras.contains(&value),
                menu::Domain::Costumes
                | menu::Domain::Backdrops
                | menu::Domain::Sounds
                | menu::Domain::Fixed(_)
                | menu::Domain::Open => false,
            };
            if allowed {
                return Ok(value.to_string());
            }
            // A sprite name is also a valid target in a sprite menu.
            if matches!(domain, menu::Domain::Sprites(_)) {
                return Ok(value.to_string());
            }
        }
        // A sprite declared in the project is a valid target.
        if matches!(domain, menu::Domain::Sprites(_)) {
            for sprite in &self.globals.sprite_names {
                if menu::variant(sprite) == variant {
                    return Ok(sprite.clone());
                }
            }
        }
        Err(Error::new(
            source
                .error(span.pos, format!("`{variant}` is not a value of this menu"))
                .span(span.len)
                .note(format!("the values are {}", self.variant_list(menu_id))),
        ))
    }

    fn variant_list(&self, menu_id: &str) -> String {
        let names: Vec<String> = match menu::domain(menu_id) {
            menu::Domain::Fixed(values) => values.iter().map(|v| menu::variant(v)).collect(),
            menu::Domain::Sprites(_) => {
                let mut names: Vec<String> = self
                    .globals
                    .sprite_names
                    .iter()
                    .map(|s| menu::variant(s))
                    .collect();
                names.extend(
                    [
                        "MousePointer",
                        "EdgeOfStage",
                        "RandomPosition",
                        "Myself",
                        "Stage",
                    ]
                    .iter()
                    .map(|s| (*s).to_string()),
                );
                names
            }
            menu::Domain::Costumes => self.costumes.iter().map(|c| menu::variant(c)).collect(),
            menu::Domain::Backdrops => self
                .globals
                .stage_costumes
                .iter()
                .map(|c| menu::variant(c))
                .collect(),
            menu::Domain::Sounds => self.sounds.iter().map(|c| menu::variant(c)).collect(),
            menu::Domain::Open => Vec::new(),
        };
        if names.is_empty() {
            "any literal".to_string()
        } else {
            names
                .iter()
                .map(|n| format!("`{}::{n}`", menu::type_name(menu_id)))
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    // -- helpers ----------------------------------------------------------

    /// `motion::move_steps` → `("motion", "move_steps")`, with an optional
    /// leading `std`.
    fn split_std(&self, path: &ast::Path) -> Option<(String, String)> {
        let segments: Vec<&str> = path.segments.iter().map(|s| s.name.as_str()).collect();
        let (module, name) = match segments.as_slice() {
            [module, name] => (*module, *name),
            ["std", module, name] => (*module, *name),
            _ => return None,
        };
        if module == CONSOLE_MODULE {
            return Some((module.to_string(), name.to_string()));
        }
        if !stdlib::modules().contains(&module) {
            return None;
        }
        Some((module.to_string(), name.to_string()))
    }

    /// Resolve a call's callee: a qualified `module::name`, or a bare name that
    /// is unambiguous across the whole standard library.
    fn resolve_std(&self, path: &ast::Path, source: &Source) -> Result<Option<(String, String)>> {
        if let Some(pair) = self.split_std(path) {
            return Ok(Some(pair));
        }
        if !path.is_single() {
            return Ok(None);
        }
        let name = &path.last().name;
        let modules: Vec<&'static str> = stdlib::BINDINGS
            .iter()
            .filter_map(|row| match row.binding {
                Binding::Function {
                    module,
                    name: row_name,
                    ..
                } if row_name == name => Some(module),
                _ => None,
            })
            .collect();
        match modules.as_slice() {
            [] => Ok(None),
            [module] => Ok(Some(((*module).to_string(), name.clone()))),
            many => {
                let mut spellings: Vec<String> = many
                    .iter()
                    .map(|module| format!("`{module}::{name}`"))
                    .collect();
                spellings.sort();
                Err(Error::new(
                    source
                        .error(
                            path.span.pos,
                            format!(
                                "`{name}` is ambiguous; the standard library has {}",
                                spellings.join(" and ")
                            ),
                        )
                        .span(path.span.len.max(1))
                        .note("write the module: the qualified form is the one that always works"),
                ))
            }
        }
    }

    fn path_name(&self, expr: &Expr, what: &str, source: &Source) -> Result<String> {
        match expr {
            Expr::Name(path) if path.is_single() => Ok(path.last().name.clone()),
            _ => Err(Error::new(
                source
                    .error(
                        expr.span().pos,
                        format!("expected {what}, found {}", describe_expr(expr)),
                    )
                    .span(expr.span().len),
            )),
        }
    }

    /// The cell a target-local scalar lives in.
    fn cell_of(&self, name: &str, source: &Source, span: Span) -> Result<usize> {
        self.locals
            .iter()
            .find(|v| v.name == name && !v.is_list)
            .map(|v| v.cell)
            .ok_or_else(|| {
                Error::new(
                    source
                        .error(span.pos, format!("there is no variable called `{name}`"))
                        .span(span.len)
                        .note("declare it with `var name: num = 0;`"),
                )
            })
    }

    /// The declared type of a target-level name, list or scalar.
    fn declared_ty(&self, name: &str) -> Option<Ty> {
        self.locals
            .iter()
            .find(|v| v.name == name)
            .map(|v| v.ty)
            .or_else(|| self.globals.var(name).map(|v| v.ty))
    }

    /// The address and type a *place* expression names.
    ///
    /// A place is a name, or a field of a place. It is not a value: reading one
    /// is a cell read, writing one is a cell write, and the cell index is a
    /// constant the compiler decided when the struct was laid out.
    fn place(&self, expr: &Expr, source: &Source) -> Result<(Addr, Ty)> {
        match expr {
            Expr::Name(path) if path.is_single() => {
                let name = &path.last().name;
                if let Some(binding) = self.lookup_scope(name) {
                    return Ok((binding.addr, binding.ty));
                }
                if let Some(cell) = self.globals.scalar_cell(name) {
                    let ty = self.globals.var(name).map_or(Ty::Num, |v| v.ty);
                    return Ok((
                        Addr {
                            cell,
                            arena: Arena::Global,
                        },
                        ty,
                    ));
                }
                Err(self.unknown_name(name, path.span, source))
            }
            Expr::Field { base, name, span } => {
                let (addr, ty) = self.place(base, source)?;
                let Ty::Struct(id) = ty else {
                    return Err(Error::new(
                        source
                            .error(
                                span.pos,
                                format!(
                                    "`{}` is `{}`, which has no fields",
                                    describe_expr(base),
                                    ty.name()
                                ),
                            )
                            .span(span.len.max(1)),
                    ));
                };
                let layout = self.struct_of(id, source, *span)?;
                let Some(field) = layout.field(&name.name) else {
                    let mut error = Error::new(
                        source
                            .error(
                                name.span.pos,
                                format!("`{}` has no field `{}`", layout.name, name.name),
                            )
                            .span(name.span.len.max(1)),
                    );
                    let fields: Vec<&str> = layout.fields.iter().map(|f| f.name.as_str()).collect();
                    if let Some(best) = closest(&name.name, &fields) {
                        error = error.note(format!("did you mean `{best}`?"));
                    }
                    return Err(error);
                };
                Ok((addr.offset(field.offset), field.ty))
            }
            _ => Err(Error::new(
                source
                    .error(
                        expr.span().pos,
                        format!("`{}` is not a place", describe_expr(expr)),
                    )
                    .span(expr.span().len.max(1))
                    .note("only a name, or a field of one, can be written"),
            )),
        }
    }

    /// The place an assignment target names, when it is a field path.
    fn lvalue_place(&self, target: &ast::LValue, source: &Source) -> Result<(Addr, Ty)> {
        let mut expr = Expr::Name(ast::Path::single(target.name.clone()));
        for accessor in &target.path {
            match accessor {
                ast::Accessor::Field(name) => {
                    expr = Expr::Field {
                        base: Box::new(expr),
                        name: name.clone(),
                        span: target.span,
                    };
                }
                ast::Accessor::Index(index) => {
                    expr = Expr::Index {
                        list: Box::new(expr),
                        index: index.clone(),
                        span: target.span,
                    };
                }
            }
        }
        self.place(&expr, source)
    }

    /// Write a value into the place an assignment target names.
    /// Whether `name` has a `watch`, and therefore a visible mirror to keep in
    /// step with every write to its cell.
    fn watched(&self, name: &str) -> bool {
        self.watches.contains(name) || self.globals.watches.contains(name)
    }

    fn watch_write(&self, name: &str, value: &rasm::Expr) -> Option<rasm::Stmt> {
        self.watched(name).then(|| {
            mk_stmt(
                "data_setvariableto",
                vec![mk_str(name.to_string()), value.clone()],
            )
        })
    }

    /// The item an assignment to `name[index]` names: the index, bound once
    /// when it is not pure, and the statement that makes sure that item exists.
    fn list_item_write(
        &mut self,
        name: &str,
        index: &ast::Expr,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<(rasm::Expr, Vec<rasm::Stmt>)> {
        let lowered = self.expr(index, source, params)?;
        expect(&lowered, Ty::Num, "a list index", source)?;
        let (slot, bind) = if matches!(self.purity_of(index), Purity::Pure) {
            (lowered.expr, None)
        } else {
            let (addr, init) = self.temp_cell(lowered.expr);
            (addr.read(), Some(init))
        };
        let mut out = Vec::new();
        out.extend(bind);
        out.push(list_grow(name, slot.clone()));
        Ok((slot, out))
    }

    fn assign_place(
        &mut self,
        target: &ast::LValue,
        value: Typed,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Vec<rasm::Stmt>> {
        let name = target.name.name.clone();
        self.reject_unsubstituted(&name, target.name.span, source)?;
        match target.path.as_slice() {
            [] => {
                if let Some(binding) = self.lookup_scope(&name) {
                    if !binding.ty.is_scalar() {
                        return Err(no_value_for(source, target.span, &name, binding.ty));
                    }
                    expect(&value, binding.ty, &format!("`{name}`"), source)?;
                    let mut out = vec![binding.addr.write(value.expr.clone())];
                    out.extend(self.watch_write(&name, &binding.addr.read()));
                    return Ok(out);
                }
                if let Some(cell) = self.globals.scalar_cell(&name) {
                    let ty = self.globals.var(&name).map_or(Ty::Num, |v| v.ty);
                    expect(&value, ty, &format!("`{name}`"), source)?;
                    let mut out = vec![Addr {
                        cell,
                        arena: Arena::Global,
                    }
                    .write(value.expr.clone())];
                    let mirror = Addr {
                        cell,
                        arena: Arena::Global,
                    };
                    out.extend(self.watch_write(&name, &mirror.read()));
                    return Ok(out);
                }
                let expected = self.variable_ty(&name, source, target.name.span)?;
                expect(&value, expected, &format!("`{name}`"), source)?;
                Ok(vec![Addr {
                    cell: self.cell_of(&name, source, target.name.span)?,
                    arena: Arena::Local,
                }
                .write(value.expr)])
            }
            [ast::Accessor::Index(index)] => {
                let element = self.list_element(&name, source, target.name.span)?;
                expect(&value, Ty::from(element), &format!("`{name}`"), source)?;
                let (slot, mut out) = self.list_item_write(&name, index, source, params)?;
                out.push(mk_stmt(
                    "data_replaceitemoflist",
                    vec![slot, mk_str(name), value.expr],
                ));
                Ok(out)
            }
            _ => {
                let (addr, ty) = self.lvalue_place(target, source)?;
                expect(&value, ty, &format!("`{name}`"), source)?;
                Ok(vec![addr.write(value.expr)])
            }
        }
    }

    /// `x += e` on whatever `x` is.
    fn compound_place(
        &mut self,
        target: &ast::LValue,
        opcode: &str,
        rhs: rasm::Expr,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Vec<rasm::Stmt>> {
        let name = target.name.name.clone();
        self.reject_unsubstituted(&name, target.name.span, source)?;
        match target.path.as_slice() {
            [ast::Accessor::Index(index)] => {
                let element = self.list_element(&name, source, target.name.span)?;
                // `+=` only means something for a number.
                let placeholder = Typed {
                    expr: mk_num("0"),
                    ty: Ty::from(element),
                    span: target.span,
                };
                expect(&placeholder, Ty::Num, &format!("`{name}`"), source)?;
                let (slot, mut out) = self.list_item_write(&name, index, source, params)?;
                let read = mk_call("data_itemoflist", vec![slot.clone(), mk_str(name.clone())]);
                out.push(mk_stmt(
                    "data_replaceitemoflist",
                    vec![slot, mk_str(name), mk_call(opcode, vec![read, rhs])],
                ));
                Ok(out)
            }
            [] => {
                let (addr, ty) = if let Some(binding) = self.lookup_scope(&name) {
                    (binding.addr, binding.ty)
                } else if let Some(cell) = self.globals.scalar_cell(&name) {
                    let ty = self.globals.var(&name).map_or(Ty::Num, |v| v.ty);
                    (
                        Addr {
                            cell,
                            arena: Arena::Global,
                        },
                        ty,
                    )
                } else {
                    let ty = self.variable_ty(&name, source, target.name.span)?;
                    (
                        Addr {
                            cell: self.cell_of(&name, source, target.name.span)?,
                            arena: Arena::Local,
                        },
                        ty,
                    )
                };
                let placeholder = Typed {
                    expr: mk_num("0"),
                    ty,
                    span: target.span,
                };
                expect(&placeholder, Ty::Num, &format!("`{name}`"), source)?;
                let combined = mk_call(opcode, vec![addr.read(), rhs]);
                let mut out = vec![addr.write(combined)];
                out.extend(self.watch_write(&name, &addr.read()));
                Ok(out)
            }
            _ => {
                let (addr, ty) = self.lvalue_place(target, source)?;
                let placeholder = Typed {
                    expr: mk_num("0"),
                    ty,
                    span: target.span,
                };
                expect(&placeholder, Ty::Num, &format!("`{name}`"), source)?;
                let combined = mk_call(opcode, vec![addr.read(), rhs]);
                let mut out = vec![addr.write(combined)];
                out.extend(self.watch_write(&name, &addr.read()));
                Ok(out)
            }
        }
    }

    // -- methods ----------------------------------------------------------

    /// The declared list or map a method receiver names.
    fn receiver(&mut self, expr: &Expr, source: &Rc<Source>) -> Result<(String, Ty)> {
        let Expr::Name(path) = expr else {
            return Err(Error::new(
                source
                    .error(
                        expr.span().pos,
                        "a method is called on a declared list or map",
                    )
                    .span(expr.span().len.max(1)),
            ));
        };
        if !path.is_single() {
            return Err(Error::new(source.error(
                path.span.pos,
                format!("`{}` is not a value", path.display()),
            )));
        }
        let name = path.last().name.clone();
        self.reject_unsubstituted(&name, path.span, source)?;
        let ty = self
            .declared_ty(&name)
            .ok_or_else(|| self.unknown_name(&name, path.span, source))?;
        if !ty.is_list() {
            return Err(Error::new(
                source
                    .error(
                        path.span.pos,
                        format!(
                            "`{name}` is `{}`, and methods are for lists and maps",
                            ty.name()
                        ),
                    )
                    .span(path.span.len),
            ));
        }
        Ok((name, ty))
    }

    /// `receiver.method(args);` — the VMS methods that are statements.
    fn method_stmt(
        &mut self,
        stmt: &ast::MethodStmt,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Vec<rasm::Stmt>> {
        let (name, ty) = self.receiver(&stmt.receiver, source)?;
        let method = stmt.name.name.as_str();
        let arg = |at: usize, this: &mut Self| -> Result<Typed> {
            let Some(expr) = stmt.args.get(at) else {
                return Err(method_arity(source, &stmt.name, method, ty));
            };
            this.expr(expr, source, params)
        };
        if stmt.args.len() > 2 {
            return Err(method_arity(source, &stmt.name, method, ty));
        }
        match (ty, method) {
            (Ty::List(element), "push") if stmt.args.len() == 1 => {
                let value = arg(0, self)?;
                expect(&value, Ty::from(element), "the pushed value", source)?;
                Ok(vec![mk_stmt(
                    "data_addtolist",
                    vec![value.expr, mk_str(name)],
                )])
            }
            (Ty::List(element), "insert") if stmt.args.len() == 2 => {
                let index = arg(0, self)?;
                expect(&index, Ty::Num, "the position", source)?;
                let value = arg(1, self)?;
                expect(&value, Ty::from(element), "the inserted value", source)?;
                Ok(vec![mk_stmt(
                    "data_insertatlist",
                    vec![value.expr, index.expr, mk_str(name)],
                )])
            }
            (Ty::List(_), "remove") if stmt.args.len() == 1 => {
                let index = arg(0, self)?;
                expect(&index, Ty::Num, "the position", source)?;
                Ok(vec![mk_stmt(
                    "data_deleteoflist",
                    vec![index.expr, mk_str(name)],
                )])
            }
            (Ty::List(_), "clear") if stmt.args.is_empty() => {
                Ok(vec![mk_stmt("data_deletealloflist", vec![mk_str(name)])])
            }
            (Ty::List(_), "pop") if stmt.args.is_empty() => Ok(vec![mk_stmt(
                "data_deleteoflist",
                vec![
                    mk_call("data_lengthoflist", vec![mk_str(name.clone())]),
                    mk_str(name),
                ],
            )]),
            (Ty::Map(..), "clear") if stmt.args.is_empty() => {
                Ok(vec![mk_stmt("data_deletealloflist", vec![mk_str(name)])])
            }
            (Ty::Map(key, value), "set") if stmt.args.len() == 2 => {
                let k = arg(0, self)?;
                expect(&k, Ty::from(key), "the key", source)?;
                let v = arg(1, self)?;
                expect(&v, Ty::from(value), "the value", source)?;
                let (slot, init) = self.temp_cell(mk_call(
                    "data_itemnumoflist",
                    vec![k.expr.clone(), mk_str(name.clone())],
                ));
                let found = slot.read();
                // Where the key is, once: the `if` and the write both need it.
                let mut out = vec![init];
                let yes = vec![mk_stmt(
                    "data_replaceitemoflist",
                    vec![
                        mk_call("operator_add", vec![found.clone(), mk_num("1")]),
                        mk_str(name.clone()),
                        v.expr.clone(),
                    ],
                )];
                let no = vec![
                    mk_stmt("data_addtolist", vec![k.expr, mk_str(name.clone())]),
                    mk_stmt("data_addtolist", vec![v.expr, mk_str(name.clone())]),
                ];
                out.push(mk_block_else(
                    "control_if_else",
                    vec![mk_call("operator_gt", vec![found, mk_num("0")])],
                    yes,
                    no,
                ));
                Ok(out)
            }
            (Ty::Map(key, _), "remove") if stmt.args.len() == 1 => {
                let k = arg(0, self)?;
                expect(&k, Ty::from(key), "the key", source)?;
                let (slot, init) = self.temp_cell(mk_call(
                    "data_itemnumoflist",
                    vec![k.expr.clone(), mk_str(name.clone())],
                ));
                let found = slot.read();
                let mut out = vec![init];
                // The value follows its key, so deleting the key twice removes
                // the pair.
                let yes = vec![
                    mk_stmt(
                        "data_deleteoflist",
                        vec![found.clone(), mk_str(name.clone())],
                    ),
                    mk_stmt(
                        "data_deleteoflist",
                        vec![found.clone(), mk_str(name.clone())],
                    ),
                ];
                out.push(mk_block(
                    "control_if",
                    vec![mk_call("operator_gt", vec![found, mk_num("0")])],
                    yes,
                ));
                Ok(out)
            }
            _ => Err(no_such_method(source, &stmt.name, method, ty, false)),
        }
    }

    /// `receiver.method(args)` — the VMS methods that are values.
    fn method_expr(
        &mut self,
        expr: &Expr,
        receiver: &Expr,
        name: &Ident,
        args: &[Expr],
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Typed> {
        let (list, ty) = self.receiver(receiver, source)?;
        let method = name.name.as_str();
        let value_of = |this: &mut Self, at: usize| -> Result<Typed> {
            let Some(arg) = args.get(at) else {
                return Err(method_arity(source, name, method, ty));
            };
            this.expr(arg, source, params)
        };
        match (ty, method, args.len()) {
            (Ty::List(_), "len", 0) => Ok(Typed {
                expr: mk_call("data_lengthoflist", vec![mk_str(list)]),
                ty: Ty::Num,
                span: expr.span(),
            }),
            (Ty::List(element), "contains", 1) => {
                let value = value_of(self, 0)?;
                expect(&value, Ty::from(element), "the value", source)?;
                Ok(Typed {
                    expr: mk_call("data_listcontainsitem", vec![mk_str(list), value.expr]),
                    ty: Ty::Bool,
                    span: expr.span(),
                })
            }
            (Ty::List(element), "index_of", 1) => {
                let value = value_of(self, 0)?;
                expect(&value, Ty::from(element), "the value", source)?;
                Ok(Typed {
                    expr: mk_call("data_itemnumoflist", vec![value.expr, mk_str(list)]),
                    ty: Ty::Num,
                    span: expr.span(),
                })
            }
            (Ty::List(element), "at", 1) => {
                let index = value_of(self, 0)?;
                expect(&index, Ty::Num, "the position", source)?;
                Ok(Typed {
                    expr: read_bool(
                        Ty::from(element),
                        mk_call("data_itemoflist", vec![index.expr, mk_str(list)]),
                    ),
                    ty: Ty::from(element),
                    span: expr.span(),
                })
            }
            (Ty::List(element), "first", 0) => Ok(Typed {
                expr: read_bool(
                    Ty::from(element),
                    mk_call("data_itemoflist", vec![mk_num("1"), mk_str(list)]),
                ),
                ty: Ty::from(element),
                span: expr.span(),
            }),
            // The last item is the one at the list's length, and Scratch's
            // `item of` computes that at run time — one reporter, not a loop.
            (Ty::List(element), "last", 0) => Ok(Typed {
                expr: read_bool(
                    Ty::from(element),
                    mk_call(
                        "data_itemoflist",
                        vec![
                            mk_call("data_lengthoflist", vec![mk_str(list.clone())]),
                            mk_str(list),
                        ],
                    ),
                ),
                ty: Ty::from(element),
                span: expr.span(),
            }),
            // The whole list as one string, which is `data_listcontents` — the
            // same block `data::contents_of_list(l)` names.
            (Ty::List(_), "text", 0) => Ok(Typed {
                expr: mk_call("data_listcontents", vec![mk_str(list)]),
                ty: Ty::Str,
                span: expr.span(),
            }),
            (Ty::List(_), "is_empty", 0) => Ok(Typed {
                expr: mk_call(
                    "operator_equals",
                    vec![
                        mk_call("data_lengthoflist", vec![mk_str(list)]),
                        mk_num("0"),
                    ],
                ),
                ty: Ty::Bool,
                span: expr.span(),
            }),
            (Ty::Map(..), "is_empty", 0) => Ok(Typed {
                expr: mk_call(
                    "operator_equals",
                    vec![
                        mk_call("data_lengthoflist", vec![mk_str(list)]),
                        mk_num("0"),
                    ],
                ),
                ty: Ty::Bool,
                span: expr.span(),
            }),
            (Ty::Map(..), "len", 0) => Ok(Typed {
                expr: mk_call(
                    "operator_divide",
                    vec![
                        mk_call("data_lengthoflist", vec![mk_str(list)]),
                        mk_num("2"),
                    ],
                ),
                ty: Ty::Num,
                span: expr.span(),
            }),
            (Ty::Map(key, _), "has", 1) => {
                let k = value_of(self, 0)?;
                expect(&k, Ty::from(key), "the key", source)?;
                Ok(Typed {
                    expr: mk_call(
                        "operator_gt",
                        vec![
                            mk_call("data_itemnumoflist", vec![k.expr, mk_str(list)]),
                            mk_num("0"),
                        ],
                    ),
                    ty: Ty::Bool,
                    span: expr.span(),
                })
            }
            (Ty::Map(key, value), "get", 1) => {
                let k = value_of(self, 0)?;
                expect(&k, Ty::from(key), "the key", source)?;
                // A missing key has position zero, and `item 1 of` a list is
                // its first *key*, so the read is guarded by a test. The two
                // cells are the cost of a table whose keys are not indices.
                let (found, found_init) = self.temp_cell(mk_call(
                    "data_itemnumoflist",
                    vec![k.expr, mk_str(list.clone())],
                ));
                let (slot, slot_init) = self.temp_cell(mk_str(""));
                let mut pre = vec![found_init, slot_init];
                pre.push(mk_block(
                    "control_if",
                    vec![mk_call("operator_gt", vec![found.read(), mk_num("0")])],
                    vec![slot.write(mk_call(
                        "data_itemoflist",
                        vec![
                            mk_call("operator_add", vec![found.read(), mk_num("1")]),
                            mk_str(list),
                        ],
                    ))],
                ));
                self.pre.extend(pre);
                Ok(Typed {
                    expr: read_bool(Ty::from(value), slot.read()),
                    ty: Ty::from(value),
                    span: expr.span(),
                })
            }
            _ => Err(no_such_method(source, name, method, ty, true)),
        }
    }

    fn variable_ty(&self, name: &str, source: &Source, span: Span) -> Result<Ty> {
        if let Some(var) = self.locals.iter().find(|v| v.name == name) {
            if var.is_list {
                return Err(list_where_scalar(source, span, name));
            }
            return Ok(var.ty);
        }
        if let Some(var) = self.globals.var(name) {
            if var.is_list {
                return Err(list_where_scalar(source, span, name));
            }
            return Ok(var.ty);
        }
        Err(Error::new(
            source
                .error(span.pos, format!("there is no variable called `{name}`"))
                .span(span.len)
                .note("declare it with `var name: num = 0;`"),
        ))
    }

    fn list_element(&self, name: &str, source: &Source, span: Span) -> Result<Scalar> {
        if let Some(var) = self.locals.iter().find(|v| v.name == name) {
            if let Some(element) = var.ty.element() {
                return Ok(element);
            }
        }
        if let Some(var) = self.globals.var(name) {
            if let Some(element) = var.ty.element() {
                return Ok(element);
            }
        }
        Err(Error::new(
            source
                .error(span.pos, format!("there is no list called `{name}`"))
                .span(span.len)
                .note("declare it with `var name: list<num> = [];`"),
        ))
    }

    /// How many times an expression may be evaluated, structurally.
    fn purity_of(&self, expr: &Expr) -> Purity {
        match expr {
            Expr::Number { .. } | Expr::Str { .. } | Expr::Bool { .. } => Purity::Pure,
            Expr::Param(_) => Purity::Pure,
            Expr::Name(path) => {
                if !path.is_single() {
                    return Purity::Pure;
                }
                let name = &path.last().name;
                if self.lookup_scope(name).is_some() {
                    // A `let` copies its value into its cell once, so reading the
                    // cell twice cannot disagree with itself.
                    Purity::Pure
                } else if self.consts.contains_key(name) {
                    Purity::Pure
                } else if self.locals.iter().any(|v| &v.name == name)
                    || self.globals.var(name).is_some()
                {
                    Purity::Sampled
                } else {
                    // A parameter reporter is pure.
                    Purity::Pure
                }
            }
            Expr::Call(call) => {
                if call.callee.is_single() {
                    let name = call.callee.last().name.as_str();
                    // A conversion is as pure as what it converts: it is a retype.
                    if (name == "num" || name == "str") && call.args.len() == 1 {
                        return self.purity_of(&call.args[0]);
                    }
                    // A call to an `fn` is as pure as its argument, which we
                    // cannot see here; be conservative.
                    return Purity::Sampled;
                }
                match self.split_std(&call.callee) {
                    Some((module, name)) => match stdlib::opcode_for(&module, &name) {
                        Some(opcode) => purity::of(opcode),
                        None => Purity::Sampled,
                    },
                    None => Purity::Sampled,
                }
            }
            Expr::Index { .. } => Purity::Sampled,
            Expr::Field { base, .. } => self.purity_of(base),
            Expr::Method { receiver, name, .. } => {
                // `get` writes two cells of its own, so it must not be copied;
                // every other method is as pure as what it is called on.
                if name.name == "get" {
                    Purity::Effectful
                } else {
                    self.purity_of(receiver)
                }
            }
            Expr::Struct { .. } => Purity::Effectful,
            Expr::Unary { operand, .. } => self.purity_of(operand),
            Expr::Binary { lhs, rhs, .. } => self.purity_of(lhs).max(self.purity_of(rhs)),
            Expr::Interpolated { parts, .. } => parts
                .iter()
                .map(|part| match part {
                    ast::InterpPart::Text(_) => Purity::Pure,
                    ast::InterpPart::Hole(expr) => self.purity_of(expr),
                })
                .max()
                .unwrap_or(Purity::Pure),
            Expr::Macro(_) => Purity::Sampled,
        }
    }

    // -- macros -----------------------------------------------------------

    fn expand(
        &mut self,
        call: &ast::MacroCall,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Expansion> {
        let name = call.name.name.clone();
        let Some(def) = self.macros.get(&name).cloned() else {
            return Err(Error::new(
                source
                    .error(call.name.span.pos, format!("cannot find `{name}`"))
                    .span(call.name.span.len),
            ));
        };
        if call.args.len() != def.params.len() {
            let expected: Vec<String> = def.params.iter().map(|p| p.name.name.clone()).collect();
            return Err(Error::new(
                source
                    .error(
                        call.span.pos,
                        format!(
                            "`{}` takes {} argument{}, but {} were given",
                            def.name,
                            def.params.len(),
                            if def.params.len() == 1 { "" } else { "s" },
                            call.args.len()
                        ),
                    )
                    .span(call.span.len)
                    .note(format!("{} takes {}", def.name, expected.join(", ")))
                    .note(format!("it is declared as {}", def.signature())),
            ));
        }
        // A macro is refused when its *definition* reaches itself. A call that
        // arrives through a substituted argument is the caller's code, not a
        // step of this expansion, so `for` inside `for` is not a cycle.
        if let Some(chain) = self.macro_cycles.get(&name) {
            return Err(Error::new(
                source
                    .error(
                        call.name.span.pos,
                        format!("`{name}` expands into itself"),
                    )
                    .span(call.name.span.len)
                    .note(format!("the cycle is {}", chain.join(" → ")))
                    .note("macro expansion must be acyclic; that is what makes every program cost a decidable number of blocks"),
            ));
        }
        if self.macro_depth >= MAX_EXPANSION_DEPTH {
            return Err(Error::new(
                source
                    .error(call.name.span.pos, format!("`{name}` expands too deeply"))
                    .span(call.name.span.len)
                    .note(format!("macro expansion may not nest more than {MAX_EXPANSION_DEPTH} deep"))
                    .note("macro expansion must be acyclic; that is what makes every program cost a decidable number of blocks"),
            ));
        }

        // Check the arguments against the declared parameter kinds.
        let mut subs: HashMap<String, ast::MacroArg> = HashMap::new();
        for (param, written) in def.params.iter().zip(call.args.iter()) {
            // A name written where the macro wants an `ident` is a name, not an
            // expression: `repeat_counted(step, 5)` reads better than making the
            // caller remember which parameters are mentions.
            let arg = &coerce_arg(&param.kind, written);
            let ok = matches!(
                (&param.kind, arg),
                (ast::MacroParamKind::Block, MacroArg::Block(_))
                    | (ast::MacroParamKind::Ident, MacroArg::Ident(_))
                    | (ast::MacroParamKind::Expr(_), MacroArg::Expr(_))
            );
            if !ok {
                return Err(Error::new(
                    source
                        .error(
                            macro_arg_span(arg).pos,
                            format!(
                                "`{}` expects {} for `${}`, found {}",
                                def.name,
                                param.kind.describe(),
                                param.name.name,
                                describe_arg(arg)
                            ),
                        )
                        .span(macro_arg_span(arg).len),
                ));
            }
            if let (ast::MacroParamKind::Expr(Some(want)), MacroArg::Expr(expr)) =
                (&param.kind, arg)
            {
                if self.macros.contains_key(&name) {
                    // The argument is lowered again where it is substituted, so
                    // this check must not leave effects behind.
                    let mark = self.pre.len();
                    let found = self.expr(expr, source, params)?.ty;
                    self.pre.truncate(mark);
                    if found != *want {
                        return Err(Error::new(
                            source
                                .error(
                                    expr.span().pos,
                                    format!(
                                        "`{}` expects `{}` for `${}`, found `{}`",
                                        def.name,
                                        want.name(),
                                        param.name.name,
                                        found.name()
                                    ),
                                )
                                .span(expr.span().len)
                                .note(format!("it is declared as {}", def.signature()))
                                .note(def.origin_note()),
                        ));
                    }
                }
            }
            subs.insert(param.name.name.clone(), arg.clone());
        }

        // A substitution used in more than one statement of the expansion must
        // be safe to evaluate more than once.
        if let MacroBody::Stmts(body) = &def.body {
            for param in &def.params {
                if !matches!(param.kind, ast::MacroParamKind::Expr(_)) {
                    continue;
                }
                let uses = body
                    .iter()
                    .filter(|stmt| mentions_stmt(stmt, &param.name.name))
                    .count();
                if uses <= 1 {
                    continue;
                }
                let Some(MacroArg::Expr(expr)) = subs.get(&param.name.name) else {
                    continue;
                };
                let purity = self.purity_of(expr);
                if purity.may_be_duplicated() {
                    continue;
                }
                return Err(Error::new(
                    source
                        .error(
                            expr.span().pos,
                            format!(
                                "`{}` expands `${}` into {uses} statements, and {} is {}",
                                def.name,
                                param.name.name,
                                describe_expr(expr),
                                purity.name()
                            ),
                        )
                        .span(expr.span().len)
                        .note(format!("it is declared as {}", def.signature()))
                        .note("bind it once with `let`, or pass something the compiler can copy"),
                ));
            }
        }

        let renames = self.renames(&def);
        match &def.body {
            MacroBody::Expr(expr) => Ok(Expansion::Expr(subst_expr(expr, &subs, &renames, &def))),
            MacroBody::Stmts(body) => {
                let mut out = Vec::new();
                for stmt in body {
                    out.extend(subst_stmt(stmt, &subs, &renames, &def));
                }
                Ok(Expansion::Stmts(out))
            }
        }
    }

    /// Lower an expanded macro body: one nested expansion deeper, and inside a
    /// macro, so a `var` statement and a `$name` target are allowed.
    fn lower_macro_stmts(
        &mut self,
        stmts: &[Stmt],
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Vec<rasm::Stmt>> {
        self.macro_depth += 1;
        let saved = self.in_macro;
        self.in_macro = true;
        let result = self.stmts(stmts, source, params);
        self.in_macro = saved;
        self.macro_depth -= 1;
        result
    }

    /// Lower an expanded macro expression, one nested expansion deeper.
    fn lower_macro_expr(
        &mut self,
        expr: &Expr,
        source: &Rc<Source>,
        params: &[(String, Scalar)],
    ) -> Result<Typed> {
        self.macro_depth += 1;
        let result = self.expr(expr, source, params);
        self.macro_depth -= 1;
        result
    }

    /// The names the macro body introduces, mapped to fresh ones.
    ///
    /// This is what makes a macro hygienic: a name it declares is its own, and
    /// two expansions of the same macro cannot share a variable.
    fn renames(&mut self, def: &MacroDef) -> HashMap<String, String> {
        let mut declared: Vec<String> = Vec::new();
        collect_declared(&def.body, &mut declared);
        self.hygiene += 1;
        let index = self.hygiene;
        let mut map = HashMap::new();
        for name in declared {
            // A `$name` is either supplied by the caller — in which case it was
            // substituted away already — or a lowering error. Either way it is
            // not a temporary, so it is never renamed.
            if name.starts_with('$') {
                continue;
            }
            map.insert(name.clone(), format!("{name}__{}_{index}", def.name));
        }
        map
    }
}

// ---------------------------------------------------------------------------
// Substitution
// ---------------------------------------------------------------------------

fn subst_expr(
    expr: &Expr,
    subs: &HashMap<String, ast::MacroArg>,
    renames: &HashMap<String, String>,
    def: &MacroDef,
) -> Expr {
    match expr {
        Expr::Param(ident) => match subs.get(&ident.name) {
            Some(MacroArg::Expr(expr)) => expr.clone(),
            Some(MacroArg::Ident(found)) => Expr::Name(ast::Path::single(Ident::new(
                found.name.clone(),
                found.span,
            ))),
            _ => Expr::Param(ident.clone()),
        },
        Expr::Name(path) => {
            let mut path = path.clone();
            if let Some(last) = path.segments.last_mut() {
                if let Some(fresh) = renames.get(&last.name) {
                    last.name = fresh.clone();
                }
            }
            Expr::Name(path)
        }
        Expr::Number { .. } | Expr::Str { .. } | Expr::Bool { .. } => expr.clone(),
        Expr::Call(call) => Expr::Call(ast::CallExpr {
            callee: rename_path(&call.callee, renames),
            args: call
                .args
                .iter()
                .map(|arg| subst_expr(arg, subs, renames, def))
                .collect(),
            span: call.span,
        }),
        Expr::Index { list, index, span } => Expr::Index {
            list: Box::new(subst_expr(list, subs, renames, def)),
            index: Box::new(subst_expr(index, subs, renames, def)),
            span: *span,
        },
        Expr::Unary { op, operand, span } => Expr::Unary {
            op: *op,
            operand: Box::new(subst_expr(operand, subs, renames, def)),
            span: *span,
        },
        Expr::Binary { op, lhs, rhs, span } => Expr::Binary {
            op: *op,
            lhs: Box::new(subst_expr(lhs, subs, renames, def)),
            rhs: Box::new(subst_expr(rhs, subs, renames, def)),
            span: *span,
        },
        Expr::Interpolated { parts, span } => Expr::Interpolated {
            parts: parts
                .iter()
                .map(|part| match part {
                    ast::InterpPart::Text(text) => ast::InterpPart::Text(text.clone()),
                    ast::InterpPart::Hole(expr) => {
                        ast::InterpPart::Hole(subst_expr(expr, subs, renames, def))
                    }
                })
                .collect(),
            span: *span,
        },
        Expr::Macro(call) => Expr::Macro(subst_macro_call(call, subs, renames, def)),
        Expr::Field { base, name, span } => Expr::Field {
            base: Box::new(subst_expr(base, subs, renames, def)),
            name: name.clone(),
            span: *span,
        },
        Expr::Method {
            receiver,
            name,
            args,
            span,
        } => Expr::Method {
            receiver: Box::new(subst_expr(receiver, subs, renames, def)),
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| subst_expr(arg, subs, renames, def))
                .collect(),
            span: *span,
        },
        Expr::Struct { name, fields, span } => Expr::Struct {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(field, value)| (field.clone(), subst_expr(value, subs, renames, def)))
                .collect(),
            span: *span,
        },
    }
}

fn subst_macro_call(
    call: &ast::MacroCall,
    subs: &HashMap<String, ast::MacroArg>,
    renames: &HashMap<String, String>,
    def: &MacroDef,
) -> ast::MacroCall {
    ast::MacroCall {
        name: call.name.clone(),
        args: call
            .args
            .iter()
            .map(|arg| match arg {
                MacroArg::Expr(expr) => MacroArg::Expr(subst_expr(expr, subs, renames, def)),
                MacroArg::Block(body) => MacroArg::Block(
                    body.iter()
                        .flat_map(|stmt| subst_stmt(stmt, subs, renames, def))
                        .collect(),
                ),
                MacroArg::Ident(ident) => MacroArg::Ident(ident.clone()),
                MacroArg::Param(ident) => match subs.get(&ident.name) {
                    Some(MacroArg::Ident(found)) => MacroArg::Ident(found.clone()),
                    Some(MacroArg::Expr(expr)) => MacroArg::Expr(expr.clone()),
                    _ => MacroArg::Param(ident.clone()),
                },
            })
            .collect(),
        span: call.span,
    }
}

/// A `$body` in statement position becomes the statements it stands for.
fn subst_stmt(
    stmt: &Stmt,
    subs: &HashMap<String, ast::MacroArg>,
    renames: &HashMap<String, String>,
    def: &MacroDef,
) -> Vec<Stmt> {
    match stmt {
        Stmt::Param(ident) => match subs.get(&ident.name) {
            Some(MacroArg::Block(body)) => body.clone(),
            _ => Vec::new(),
        },
        Stmt::Let(decl) => vec![Stmt::Let(ast::LetStmt {
            name: subst_ident(&decl.name, subs, renames),
            ty: decl.ty,
            value: subst_expr(&decl.value, subs, renames, def),
            span: decl.span,
        })],
        Stmt::Var(decl) => {
            vec![Stmt::Var(ast::VarDecl {
                public: decl.public,
                name: subst_ident(&decl.name, subs, renames),
                ty: decl.ty,
                init: decl.init.clone(),
                span: decl.span,
            })]
        }
        Stmt::Assign(assign) => vec![Stmt::Assign(ast::AssignStmt {
            target: subst_lvalue(&assign.target, subs, renames, def),
            value: subst_expr(&assign.value, subs, renames, def),
            span: assign.span,
        })],
        Stmt::CompoundAssign(assign) => vec![Stmt::CompoundAssign(ast::CompoundAssignStmt {
            target: subst_lvalue(&assign.target, subs, renames, def),
            op: assign.op,
            value: subst_expr(&assign.value, subs, renames, def),
            span: assign.span,
        })],
        Stmt::Return(ret) => vec![Stmt::Return(ast::ReturnStmt {
            value: ret
                .value
                .as_ref()
                .map(|value| subst_expr(value, subs, renames, def)),
            span: ret.span,
        })],
        Stmt::Call(call) => vec![Stmt::Call(ast::CallStmt {
            callee: rename_path(&call.callee, renames),
            args: call
                .args
                .iter()
                .map(|arg| subst_expr(arg, subs, renames, def))
                .collect(),
            body: call.body.as_ref().map(|body| {
                body.iter()
                    .flat_map(|stmt| subst_stmt(stmt, subs, renames, def))
                    .collect()
            }),
            body_span: call.body_span,
            span: call.span,
        })],
        Stmt::If(if_stmt) => vec![Stmt::If(ast::IfStmt {
            cond: subst_expr(&if_stmt.cond, subs, renames, def),
            then_branch: flatten(&if_stmt.then_branch, subs, renames, def),
            else_branch: if_stmt
                .else_branch
                .as_ref()
                .map(|body| flatten(body, subs, renames, def)),
            span: if_stmt.span,
        })],
        Stmt::Loop(loop_stmt) => vec![Stmt::Loop(ast::LoopStmt {
            kind: match &loop_stmt.kind {
                ast::LoopKind::Repeat(expr) => {
                    ast::LoopKind::Repeat(subst_expr(expr, subs, renames, def))
                }
                ast::LoopKind::RepeatUntil(expr) => {
                    ast::LoopKind::RepeatUntil(subst_expr(expr, subs, renames, def))
                }
                ast::LoopKind::Forever => ast::LoopKind::Forever,
            },
            body: flatten(&loop_stmt.body, subs, renames, def),
            span: loop_stmt.span,
        })],
        Stmt::Match(match_stmt) => vec![Stmt::Match(ast::MatchStmt {
            subject: subst_expr(&match_stmt.subject, subs, renames, def),
            arms: match_stmt
                .arms
                .iter()
                .map(|arm| ast::MatchArm {
                    pattern: arm.pattern.clone(),
                    body: flatten(&arm.body, subs, renames, def),
                    span: arm.span,
                })
                .collect(),
            span: match_stmt.span,
        })],
        Stmt::Macro(call) => vec![Stmt::Macro(subst_macro_call(call, subs, renames, def))],
        Stmt::Method(stmt) => vec![Stmt::Method(ast::MethodStmt {
            receiver: subst_expr(&stmt.receiver, subs, renames, def),
            name: stmt.name.clone(),
            args: stmt
                .args
                .iter()
                .map(|arg| subst_expr(arg, subs, renames, def))
                .collect(),
            span: stmt.span,
        })],
    }
}

/// Substitute through an assignment target's own expressions.
fn subst_lvalue(
    target: &ast::LValue,
    subs: &HashMap<String, ast::MacroArg>,
    renames: &HashMap<String, String>,
    def: &MacroDef,
) -> ast::LValue {
    ast::LValue {
        name: subst_ident(&target.name, subs, renames),
        path: target
            .path
            .iter()
            .map(|accessor| match accessor {
                ast::Accessor::Field(name) => ast::Accessor::Field(name.clone()),
                ast::Accessor::Index(index) => {
                    ast::Accessor::Index(Box::new(subst_expr(index, subs, renames, def)))
                }
            })
            .collect(),
        span: target.span,
    }
}

/// The `proc` calls named inside an assignment target.
fn collect_lvalue_calls(target: &ast::LValue, out: &mut Vec<String>) {
    for accessor in &target.path {
        if let ast::Accessor::Index(index) = accessor {
            collect_calls_expr(index, out);
        }
    }
}

/// Whether an assignment target mentions `name`.
fn mentions_lvalue(target: &ast::LValue, name: &str) -> bool {
    target.name.name == name
        || target.path.iter().any(|accessor| match accessor {
            ast::Accessor::Field(field) => field.name == name,
            ast::Accessor::Index(index) => mentions_expr(index, name),
        })
}

fn flatten(
    body: &[Stmt],
    subs: &HashMap<String, ast::MacroArg>,
    renames: &HashMap<String, String>,
    def: &MacroDef,
) -> Vec<Stmt> {
    body.iter()
        .flat_map(|stmt| subst_stmt(stmt, subs, renames, def))
        .collect()
}

fn rename_ident(ident: &Ident, renames: &HashMap<String, String>) -> Ident {
    match renames.get(&ident.name) {
        Some(fresh) => Ident::new(fresh.clone(), ident.span),
        None => ident.clone(),
    }
}

/// Substitute a name that a declaration or an assignment introduces.
///
/// A `$name` is the caller's: it is replaced by whatever the caller supplied,
/// and stays as it is when the caller supplied nothing (which lowering then
/// reports). Anything else is renamed so two expansions cannot collide.
fn subst_ident(
    ident: &Ident,
    subs: &HashMap<String, ast::MacroArg>,
    renames: &HashMap<String, String>,
) -> Ident {
    match ident.name.strip_prefix('$') {
        Some(param) => match subs.get(param) {
            Some(MacroArg::Ident(found)) => found.clone(),
            Some(MacroArg::Expr(Expr::Name(path))) if path.is_single() => path.last().clone(),
            _ => ident.clone(),
        },
        None => rename_ident(ident, renames),
    }
}

fn rename_path(path: &ast::Path, renames: &HashMap<String, String>) -> ast::Path {
    let mut path = path.clone();
    if let Some(last) = path.segments.last_mut() {
        if let Some(fresh) = renames.get(&last.name) {
            last.name = fresh.clone();
        }
    }
    path
}

/// The type a block requires of the variable it names, when it does arithmetic
/// on it. Everything else takes whatever the variable holds.
fn variable_constraint(opcode: &str) -> Option<Ty> {
    match opcode {
        "data_changevariableby" => Some(Ty::Num),
        _ => None,
    }
}

/// Turn a plain name into an `ident` argument when that is what the parameter
/// wants.
fn coerce_arg(kind: &ast::MacroParamKind, arg: &MacroArg) -> MacroArg {
    match (kind, arg) {
        (ast::MacroParamKind::Ident, MacroArg::Expr(Expr::Name(path))) if path.is_single() => {
            MacroArg::Ident(path.last().clone())
        }
        _ => arg.clone(),
    }
}

/// Turn a plain reference to an `fn` parameter into a macro parameter.
fn paramify(expr: &Expr, params: &[String]) -> Expr {
    match expr {
        Expr::Name(path) if path.is_single() && params.contains(&path.last().name) => {
            Expr::Param(path.last().clone())
        }
        Expr::Name(_) | Expr::Number { .. } | Expr::Str { .. } | Expr::Bool { .. } => expr.clone(),
        Expr::Call(call) => Expr::Call(ast::CallExpr {
            callee: call.callee.clone(),
            args: call.args.iter().map(|arg| paramify(arg, params)).collect(),
            span: call.span,
        }),
        Expr::Index { list, index, span } => Expr::Index {
            list: Box::new(paramify(list, params)),
            index: Box::new(paramify(index, params)),
            span: *span,
        },
        Expr::Unary { op, operand, span } => Expr::Unary {
            op: *op,
            operand: Box::new(paramify(operand, params)),
            span: *span,
        },
        Expr::Binary { op, lhs, rhs, span } => Expr::Binary {
            op: *op,
            lhs: Box::new(paramify(lhs, params)),
            rhs: Box::new(paramify(rhs, params)),
            span: *span,
        },
        Expr::Interpolated { parts, span } => Expr::Interpolated {
            parts: parts
                .iter()
                .map(|part| match part {
                    ast::InterpPart::Text(text) => ast::InterpPart::Text(text.clone()),
                    ast::InterpPart::Hole(expr) => ast::InterpPart::Hole(paramify(expr, params)),
                })
                .collect(),
            span: *span,
        },
        Expr::Macro(call) => Expr::Macro(ast::MacroCall {
            name: call.name.clone(),
            args: call
                .args
                .iter()
                .map(|arg| match arg {
                    MacroArg::Expr(expr) => MacroArg::Expr(paramify(expr, params)),
                    other => other.clone(),
                })
                .collect(),
            span: call.span,
        }),
        Expr::Param(_) => expr.clone(),
        Expr::Field { base, name, span } => Expr::Field {
            base: Box::new(paramify(base, params)),
            name: name.clone(),
            span: *span,
        },
        Expr::Method {
            receiver,
            name,
            args,
            span,
        } => Expr::Method {
            receiver: Box::new(paramify(receiver, params)),
            name: name.clone(),
            args: args.iter().map(|arg| paramify(arg, params)).collect(),
            span: *span,
        },
        Expr::Struct { name, fields, span } => Expr::Struct {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(field, value)| (field.clone(), paramify(value, params)))
                .collect(),
            span: *span,
        },
    }
}

/// Every procedure name a statement block mentions in call position.
fn collect_calls_block(stmts: &[Stmt], out: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Call(call) => {
                if call.callee.is_single() {
                    out.push(call.callee.last().name.clone());
                }
                for arg in &call.args {
                    collect_calls_expr(arg, out);
                }
                if let Some(body) = &call.body {
                    collect_calls_block(body, out);
                }
            }
            Stmt::Let(decl) => collect_calls_expr(&decl.value, out),
            Stmt::Assign(assign) => {
                collect_lvalue_calls(&assign.target, out);
                collect_calls_expr(&assign.value, out);
            }
            Stmt::CompoundAssign(assign) => {
                collect_lvalue_calls(&assign.target, out);
                collect_calls_expr(&assign.value, out);
            }
            Stmt::Method(stmt) => {
                collect_calls_expr(&stmt.receiver, out);
                for arg in &stmt.args {
                    collect_calls_expr(arg, out);
                }
            }
            Stmt::Return(ret) => {
                if let Some(value) = &ret.value {
                    collect_calls_expr(value, out);
                }
            }
            Stmt::If(if_stmt) => {
                collect_calls_expr(&if_stmt.cond, out);
                collect_calls_block(&if_stmt.then_branch, out);
                if let Some(body) = &if_stmt.else_branch {
                    collect_calls_block(body, out);
                }
            }
            Stmt::Loop(loop_stmt) => {
                match &loop_stmt.kind {
                    ast::LoopKind::Repeat(expr) | ast::LoopKind::RepeatUntil(expr) => {
                        collect_calls_expr(expr, out);
                    }
                    ast::LoopKind::Forever => {}
                }
                collect_calls_block(&loop_stmt.body, out);
            }
            Stmt::Match(match_stmt) => {
                collect_calls_expr(&match_stmt.subject, out);
                for arm in &match_stmt.arms {
                    collect_calls_block(&arm.body, out);
                }
            }
            Stmt::Macro(call) => {
                out.push(call.name.name.clone());
                for arg in &call.args {
                    match arg {
                        MacroArg::Expr(expr) => collect_calls_expr(expr, out),
                        MacroArg::Block(body) => collect_calls_block(body, out),
                        MacroArg::Ident(_) | MacroArg::Param(_) => {}
                    }
                }
            }
            Stmt::Var(_) | Stmt::Param(_) => {}
        }
    }
}

fn collect_calls_expr(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Call(call) => {
            if call.callee.is_single() {
                out.push(call.callee.last().name.clone());
            }
            for arg in &call.args {
                collect_calls_expr(arg, out);
            }
        }
        Expr::Index { list, index, .. } => {
            collect_calls_expr(list, out);
            collect_calls_expr(index, out);
        }
        Expr::Unary { operand, .. } => collect_calls_expr(operand, out),
        Expr::Binary { lhs, rhs, .. } => {
            collect_calls_expr(lhs, out);
            collect_calls_expr(rhs, out);
        }
        Expr::Interpolated { parts, .. } => {
            for part in parts {
                if let ast::InterpPart::Hole(expr) = part {
                    collect_calls_expr(expr, out);
                }
            }
        }
        Expr::Macro(call) => {
            out.push(call.name.name.clone());
            for arg in &call.args {
                if let MacroArg::Expr(expr) = arg {
                    collect_calls_expr(expr, out);
                }
            }
        }
        Expr::Field { base, .. } => collect_calls_expr(base, out),
        Expr::Method { receiver, args, .. } => {
            collect_calls_expr(receiver, out);
            for arg in args {
                collect_calls_expr(arg, out);
            }
        }
        Expr::Struct { fields, .. } => {
            for (_, value) in fields {
                collect_calls_expr(value, out);
            }
        }
        Expr::Number { .. }
        | Expr::Str { .. }
        | Expr::Bool { .. }
        | Expr::Name(_)
        | Expr::Param(_) => {}
    }
}

/// A call chain from `start` back to `start`, when there is one.
fn cycle_through(edges: &HashMap<String, Vec<String>>, start: &str) -> Option<Vec<String>> {
    let empty: Vec<String> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack: Vec<(String, Vec<String>)> = vec![(start.to_string(), vec![start.to_string()])];
    while let Some((node, path)) = stack.pop() {
        for next in edges.get(&node).unwrap_or(&empty) {
            if next == start {
                let mut chain = path.clone();
                chain.push(start.to_string());
                return Some(chain);
            }
            if visited.insert(next.clone()) {
                let mut extended = path.clone();
                extended.push(next.clone());
                stack.push((next.clone(), extended));
            }
        }
    }
    None
}

/// Every name a macro body declares for itself.
fn collect_declared(body: &MacroBody, out: &mut Vec<String>) {
    match body {
        MacroBody::Expr(_) => {}
        MacroBody::Stmts(stmts) => collect_declared_stmts(stmts, out),
    }
}

fn collect_declared_stmts(stmts: &[Stmt], out: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Var(decl) => out.push(decl.name.name.clone()),
            Stmt::Let(decl) => out.push(decl.name.name.clone()),
            Stmt::Call(call) => {
                if let Some(body) = &call.body {
                    collect_declared_stmts(body, out);
                }
            }
            Stmt::If(if_stmt) => {
                collect_declared_stmts(&if_stmt.then_branch, out);
                if let Some(body) = &if_stmt.else_branch {
                    collect_declared_stmts(body, out);
                }
            }
            Stmt::Loop(loop_stmt) => collect_declared_stmts(&loop_stmt.body, out),
            Stmt::Match(match_stmt) => {
                for arm in &match_stmt.arms {
                    collect_declared_stmts(&arm.body, out);
                }
            }
            Stmt::Macro(call) => {
                for arg in &call.args {
                    if let MacroArg::Block(body) = arg {
                        collect_declared_stmts(body, out);
                    }
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Typed {
    expr: rasm::Expr,
    ty: Ty,
    span: Span,
}

/// What a macro call expanded to.
enum Expansion {
    Expr(Expr),
    Stmts(Block),
}

fn expect(value: &Typed, want: Ty, what: &str, source: &Source) -> Result<()> {
    if value.ty == want {
        return Ok(());
    }
    let mut error = Error::new(
        source
            .error(
                value.span.pos,
                format!(
                    "{what} must be `{}`, found `{}`",
                    want.name(),
                    value.ty.name()
                ),
            )
            .span(value.span.len.max(1)),
    );
    if ty::conversion(value.ty, want).is_some() {
        error = error.note(format!(
            "convert it with `{}()`; the two directions Scratch coerces are free",
            want.name()
        ));
    }
    Err(error)
}

fn arity_error(
    name: &str,
    params: &[(String, Ty)],
    args: &[Expr],
    span: Span,
    source: &Source,
) -> Error {
    let list: Vec<String> = params
        .iter()
        .map(|(name, ty)| format!("{name}: {}", ty.name()))
        .collect();
    Error::new(
        source
            .error(
                span.pos,
                format!(
                    "`{name}` takes {} argument{}, but {} were given",
                    params.len(),
                    if params.len() == 1 { "" } else { "s" },
                    args.len()
                ),
            )
            .span(span.len.max(1))
            .note(format!("its signature is `{name}({})`", list.join(", "))),
    )
}

/// The diagnostic for a catalog block raven refuses to expose.
///
/// raven reaches every block in the catalog — and for these five, what it has to
/// say is *no*. They are the low-level "name a Scratch variable and set it"
/// interface, which is exactly the interface the virtual memory system exists to
/// replace.
fn forbidden_block(
    source: &Source,
    path: &ast::Path,
    module: &str,
    name: &str,
    why: &str,
) -> Error {
    Error::new(
        source
            .error(
                path.span.pos,
                format!("`{module}::{name}` is not available in raven"),
            )
            .span(path.span.len.max(1))
            .note(why.to_string())
            .note("raw Scratch variables and lists are the layer raven compiles *to*, not the layer a program writes"),
    )
}

/// `x` names something that is not one value.
fn no_value_for(source: &Source, span: Span, name: &str, ty: Ty) -> Error {
    Error::new(
        source
            .error(
                span.pos,
                format!("`{name}` is a `{}`, which is not one value", ty.name()),
            )
            .span(span.len.max(1))
            .note("a struct is a frame of cells: write a field, or build it in a declaration"),
    )
}

/// A method that does not exist on this type.
///
/// The list of methods depends on where the call sits: `pop` and `clear` are
/// statements, and offering them to an expression would promise a value that
/// does not exist.
fn no_such_method(source: &Source, span: &Ident, method: &str, ty: Ty, value: bool) -> Error {
    let methods: &str = match (ty, value) {
        (Ty::List(_), true) => {
            "`len`, `is_empty`, `at`, `first`, `last`, `text`, `contains`, `index_of`"
        }
        (Ty::List(_), false) => "`push`, `insert`, `remove`, `clear`, `pop`",
        (Ty::Map(..), true) => "`len`, `is_empty`, `get`, `has`",
        (Ty::Map(..), false) => "`set`, `remove`, `clear`",
        (_, _) => "none",
    };
    Error::new(
        source
            .error(
                span.span.pos,
                format!("`{}` has no method `{method}`", ty.name()),
            )
            .span(span.span.len.max(1))
            .note(format!(
                "the methods of `{}` that {} are {methods}",
                ty.name(),
                if value {
                    "produce a value"
                } else {
                    "are statements"
                }
            )),
    )
}

/// A method called with the wrong number of arguments.
fn method_arity(source: &Source, span: &Ident, method: &str, ty: Ty) -> Error {
    Error::new(
        source
            .error(
                span.span.pos,
                format!(
                    "`{method}` on `{}` was given the wrong number of arguments",
                    ty.name()
                ),
            )
            .span(span.span.len.max(1))
            .note("see the methods listed for this type in the documentation"),
    )
}

fn list_where_scalar(source: &Source, span: Span, name: &str) -> Error {
    Error::new(
        source
            .error(span.pos, format!("`{name}` is a list, not a variable"))
            .span(span.len)
            .note("read an item with `name[i]`, or use `data::length_of_list(name)`"),
    )
}

fn duplicate(source: &Source, name: &Ident, what: &str) -> Error {
    Error::new(source.error(
        name.span.pos,
        format!("`{}` is declared as a {what} more than once", name.name),
    ))
}

/// How a compound assignment is spelled, for a diagnostic.
fn compound_spelling(op: ast::BinOp) -> String {
    format!("{}=", op.text())
}

fn describe_arg(arg: &MacroArg) -> &'static str {
    match arg {
        MacroArg::Expr(_) => "an expression",
        MacroArg::Ident(_) => "a name",
        MacroArg::Block(_) => "a `{ … }` block",
        MacroArg::Param(_) => "a macro parameter",
    }
}

fn macro_arg_span(arg: &MacroArg) -> Span {
    match arg {
        MacroArg::Expr(expr) => expr.span(),
        MacroArg::Ident(ident) => ident.span,
        MacroArg::Block(body) => body.first().map_or(Span::default(), Stmt::span),
        MacroArg::Param(ident) => ident.span,
    }
}

fn spec_block_span(spec: &BlockSpec) -> Span {
    let _ = spec;
    Span::default()
}

/// How a block reads, for an arity error.
fn signature_of(spec: &BlockSpec) -> String {
    let args: Vec<String> = spec
        .args
        .iter()
        .map(|arg| format!("{}: {:?}", arg.name.to_lowercase(), arg.shape))
        .collect();
    format!("`{}({})`", spec.opcode, args.join(", "))
}

fn literal_expr(literal: &ast::Literal) -> rasm::Expr {
    match &literal.kind {
        ast::LiteralKind::Number(text) => mk_num(text.clone()),
        ast::LiteralKind::Str(text) => mk_str(text.clone()),
        ast::LiteralKind::Bool(value) => mk_bool(*value),
    }
}

fn describe_expr(expr: &Expr) -> String {
    match expr {
        Expr::Number { text, .. } => format!("the literal `{text}`"),
        Expr::Str { .. } => "a string literal".to_string(),
        Expr::Bool { .. } => "a boolean literal".to_string(),
        Expr::Name(path) => format!("`{}`", path.display()),
        Expr::Call(call) => format!("`{}`", call.callee.display()),
        Expr::Index { .. } => "a list read".to_string(),
        Expr::Unary { .. } | Expr::Binary { .. } => "an operator".to_string(),
        Expr::Interpolated { .. } => "an interpolated string".to_string(),
        Expr::Macro(call) => format!("`{}`", call.name.name),
        Expr::Param(ident) => format!("`${}`", ident.name),
        Expr::Field { base, name, .. } => format!("`{}.{}`", describe_expr(base), name.name),
        Expr::Method { name, .. } => format!("`{}`", name.name),
        Expr::Struct { name, .. } => format!("a `{}` literal", name.name),
    }
}

/// Whether a statement mentions a name anywhere inside it.
fn mentions_stmt(stmt: &Stmt, name: &str) -> bool {
    match stmt {
        Stmt::Let(decl) => decl.name.name == name || mentions_expr(&decl.value, name),
        Stmt::Var(decl) => decl.name.name == name,
        Stmt::Assign(assign) => {
            mentions_lvalue(&assign.target, name) || mentions_expr(&assign.value, name)
        }
        Stmt::CompoundAssign(assign) => {
            mentions_lvalue(&assign.target, name) || mentions_expr(&assign.value, name)
        }
        Stmt::Method(stmt) => {
            mentions_expr(&stmt.receiver, name)
                || stmt.args.iter().any(|arg| mentions_expr(arg, name))
        }
        Stmt::Return(ret) => ret
            .value
            .as_ref()
            .is_some_and(|value| mentions_expr(value, name)),
        Stmt::Call(call) => {
            call.args.iter().any(|arg| mentions_expr(arg, name))
                || call
                    .body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|s| mentions_stmt(s, name)))
        }
        Stmt::If(if_stmt) => {
            mentions_expr(&if_stmt.cond, name)
                || if_stmt.then_branch.iter().any(|s| mentions_stmt(s, name))
                || if_stmt
                    .else_branch
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|s| mentions_stmt(s, name)))
        }
        Stmt::Loop(loop_stmt) => {
            let head = match &loop_stmt.kind {
                ast::LoopKind::Repeat(expr) | ast::LoopKind::RepeatUntil(expr) => {
                    mentions_expr(expr, name)
                }
                ast::LoopKind::Forever => false,
            };
            head || loop_stmt.body.iter().any(|s| mentions_stmt(s, name))
        }
        Stmt::Match(match_stmt) => {
            mentions_expr(&match_stmt.subject, name)
                || match_stmt
                    .arms
                    .iter()
                    .any(|arm| arm.body.iter().any(|s| mentions_stmt(s, name)))
        }
        Stmt::Macro(call) => call.args.iter().any(|arg| mentions_arg(arg, name)),
        Stmt::Param(ident) => ident.name == name,
    }
}

fn mentions_arg(arg: &MacroArg, name: &str) -> bool {
    match arg {
        MacroArg::Expr(expr) => mentions_expr(expr, name),
        MacroArg::Ident(ident) => ident.name == name,
        MacroArg::Block(body) => body.iter().any(|stmt| mentions_stmt(stmt, name)),
        MacroArg::Param(ident) => ident.name == name,
    }
}

fn mentions_expr(expr: &Expr, name: &str) -> bool {
    match expr {
        Expr::Name(path) => path.is_single() && path.last().name == name,
        Expr::Call(call) => call.args.iter().any(|arg| mentions_expr(arg, name)),
        Expr::Index { list, index, .. } => mentions_expr(list, name) || mentions_expr(index, name),
        Expr::Unary { operand, .. } => mentions_expr(operand, name),
        Expr::Binary { lhs, rhs, .. } => mentions_expr(lhs, name) || mentions_expr(rhs, name),
        Expr::Interpolated { parts, .. } => parts.iter().any(|part| match part {
            ast::InterpPart::Text(_) => false,
            ast::InterpPart::Hole(expr) => mentions_expr(expr, name),
        }),
        Expr::Macro(call) => call.args.iter().any(|arg| mentions_arg(arg, name)),
        Expr::Param(ident) => ident.name == name,
        Expr::Field { base, .. } => mentions_expr(base, name),
        Expr::Method { receiver, args, .. } => {
            mentions_expr(receiver, name) || args.iter().any(|arg| mentions_expr(arg, name))
        }
        Expr::Struct { fields, .. } => fields.iter().any(|(_, value)| mentions_expr(value, name)),
        Expr::Number { .. } | Expr::Str { .. } | Expr::Bool { .. } => false,
    }
}

/// The closest name in `candidates`, for "did you mean".
fn closest<'c>(name: &str, candidates: &[&'c str]) -> Option<&'c str> {
    let mut best: Option<(usize, &str)> = None;
    for candidate in candidates {
        let distance = levenshtein(name, candidate);
        if distance <= 2 && best.is_none_or(|(d, _)| distance < d) {
            best = Some((distance, candidate));
        }
    }
    best.map(|(_, name)| name)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current[j + 1] = (previous[j] + cost)
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

/// A path from `from` to `to`, worked out lexically.
///
/// Neither path need exist: the emitted tree is written after the source is
/// lowered, so the compiler cannot ask the file system where anything is. Both
/// are built from the project root, so comparing components is enough.
fn relative(from: &Path, to: &Path) -> String {
    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();
    let mut common = 0;
    while common < from_components.len()
        && common < to_components.len()
        && from_components[common] == to_components[common]
    {
        common += 1;
    }
    let mut out = PathBuf::new();
    for _ in common..from_components.len() {
        out.push("..");
    }
    for component in &to_components[common..] {
        out.push(component.as_os_str());
    }
    if out.as_os_str().is_empty() {
        ".".to_string()
    } else {
        out.display().to_string().replace('\\', "/")
    }
}
