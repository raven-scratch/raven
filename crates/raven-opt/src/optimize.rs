//! The optimiser: a rewrite of a raven-asm tree that removes blocks without
//! changing what the project does.
//!
//! # Why this is its own crate and not a compiler flag
//!
//! raven-asm's whole promise is *one statement, one Scratch block*: what you
//! write is what the editor shows, and a reader can count the blocks. A compiler
//! that folded behind the reader's back would break that, so `raven-asm build`
//! has no way to ask for this at all -- the optimiser is a separate program over
//! a separate input, a raven-asm project in and a raven-asm project out, and
//! running it is a decision the reader makes by running it.
//!
//! `raven` runs it for you, because the layer above is the one that hides things
//! on purpose: its macros are documented lowerings and `raven expand` prints
//! them. The optimiser is the same trade at the same layer -- a lowering raven
//! chose, not one raven-asm performed -- and `raven build --no-optimize` is how
//! to see the lowering without it.
//!
//! # What it does, and why only that
//!
//! Every rewrite here is one this module can *prove* preserves the program, and
//! the proof is the same shape for both: **an identity in Scratch's own
//! semantics, applied to operands the compiler already knows.** `operator_add(2,
//! 3)` is `5` because the VM's `add` is JavaScript's `+` on `Number()` and both
//! operands are literals; `if <false> { … }` runs nothing because an empty
//! substack is what the block does. Neither depends on what the program means,
//! only on what the block does -- which is the property that makes a rewrite
//! safe to make without reading the program.
//!
//! A rewrite that is merely *usually* right is not here. In particular:
//!
//! * **A variable is not a value.** `x` may be assigned between two uses, so a
//!   pass that propagated it would change the program. Nothing here propagates
//!   a variable, and the pass has no substitution of names at all.
//! * **A sampled reporter is not pure.** `sensing_timer()`, `looks_size()` and
//!   the whole sensing category answer differently at different moments, so
//!   neither folding nor duplicating them is sound. [`is_pure`] is the table.
//! * **Blocks are not removed for being unreachable-looking.** `if <false>` is
//!   folded, because `<false>` is a literal the compiler put there; a condition
//!   that merely looks constant is not.
//! * **A boolean is not a literal.** Scratch has no boolean literal block, so a
//!   folded boolean is spelled as `operator_equals` between two literals. The
//!   consequence is that folding a condition does *not* always save a block, and
//!   [`Const::to_expr`] says why that is the right trade anyway.
//!
//! # What was tried and is not here: inlining, and reporter substitution
//!
//! Two rewrites a layer like this would obviously make are **not** in this file,
//! and both were measured rather than assumed.
//!
//! ## Procedure inlining
//!
//! The obvious candidate, because a Scratch custom-block call is expensive. A
//! version was written and measured and it is not here:
//!
//! * **It does not pay on this repository's own projects.** With the full guard
//!   set -- one-statement body, each parameter read at most once, pure arguments,
//!   `warp` compatible, and a strict check that the body is cheaper than the
//!   call plus its arguments -- it inlined **zero** calls across `desktop`,
//!   `chess`, `sudoku`, `penfont` and `case`. Without that last check it inlined
//!   four calls in `desktop` and *added* 69 blocks, which is a pessimisation
//!   wearing an optimiser's name.
//! * **The dynamic cost is not where the static cost is.** `cpu_translate` has 37
//!   static call sites and 16.5 million dynamic calls; inlining it would
//!   duplicate a large body into 37 places and change nothing about the inner
//!   loop that pays. The wins found by `tools/profile-blocks.mjs` in this
//!   repository came from *removing the calls*, which was a source-level
//!   restructuring and not a rewrite a compiler can make.
//! * **A `warp` procedure cannot be inlined into a yielding caller.** `warp`
//!   says the runtime will not interrupt the thread; a copy of the body runs
//!   under the caller's schedule and can be interrupted. That is observable, so
//!   the guard is necessary -- and it rules out most of this repository's
//!   procedures, which are `warp` throughout.
//!
//! ## Reporter substitution
//!
//! Substituting a reporter into the place that reads it is the other obvious
//! one, and this is why it is impossible here rather than merely unprofitable.
//!
//! **A raven local is a cell of a shared list, not a value.** `crates/raven`
//! lowers every `let`, `for` counter and procedure return into one project-wide
//! list, `_vms`, addressed by a compile-time constant index, and a read of a
//! local is a `data_itemoflist` of that cell. In the `Machine` sprite of the
//! desktop example there are **1,144 cell reads against 976 cell writes** -- so
//! the cells are mutated, and a pass that replaced a read with the value some
//! earlier write computed would be propagating a value across a later write.
//! That is exactly the variable propagation this module refuses on principle; a
//! cell is no more a value than a variable is.
//!
//! The one case that looks safe is a cell written and read once with nothing in
//! between, and recognising it needs dataflow over the whole arena -- every
//! `control_repeat`, every `procedures_call` and every `control_stop` is a place
//! the analysis has to be right about. That is a much larger and much riskier
//! pass than everything else in this file, for a saving that has not been shown
//! to exist. It is not here.

use raven_asm::ast::*;
use raven_scratch::catalog;
use raven_scratch::diag::Pos;

// ---------------------------------------------------------------------------
// Purity
// ---------------------------------------------------------------------------

/// Reporters and booleans whose answer cannot change between two evaluations.
///
/// This is the mirror of `crates/raven/src/purity.rs`, which the *front end*
/// uses to decide whether a source-level alias may be duplicated. The two must
/// agree, because they answer one question — "may this be read twice?" — and a
/// disagreement would mean the optimiser duplicating something raven refused to.
/// `SAMPLED` there is the authority; this is the list read the other way round,
/// and the test at the bottom of this file holds the two in step by checking
/// that every opcode raven calls sampled is not pure here.
///
/// The rule is the catalog's kind plus this list: an `operator_*` is a function
/// of its inputs, a data reporter reads a variable (which may be assigned), and
/// the sensing category reads the world.
pub fn is_pure(opcode: &str) -> bool {
    // A procedure call is a reporter of its own, resolved by the front end;
    // raven never emits one in expression position, so it is not pure by default.
    let Some(spec) = catalog::block(opcode) else {
        return false;
    };
    if !spec.kind.is_value() {
        return false;
    }
    // Anything the front end calls sampled is sampled here.
    if sampled_opcodes().contains(&opcode) {
        return false;
    }
    // A variable or list reporter reads state that a script can change, so it is
    // not a function of its inputs alone.
    if spec.opcode.starts_with("data_variable")
        || spec.opcode.starts_with("data_listcontents")
        || spec.opcode.starts_with("data_itemof")
        || spec.opcode.starts_with("data_lengthoflist")
        || spec.opcode.starts_with("data_itemnumoflist")
        || spec.opcode.starts_with("data_listcontainsitem")
    {
        return false;
    }
    true
}

/// The opcodes `crates/raven/src/purity.rs` classifies as sampled.
///
/// Kept as a list rather than imported, because this crate must not depend on
/// `raven` — the dependency rule in AGENTS.md points right only, and `raven`
/// depends on *this* crate. A test in the `raven` crate checks the two lists
/// against each other, so they cannot drift silently.
pub fn sampled_opcodes() -> &'static [&'static str] {
    &[
        "motion_xposition",
        "motion_yposition",
        "motion_direction",
        "looks_size",
        "looks_costumenumbername",
        "looks_backdropnumbername",
        "sound_volume",
        "sensing_touchingobject",
        "sensing_touchingcolor",
        "sensing_coloristouchingcolor",
        "sensing_distanceto",
        "sensing_answer",
        "sensing_keypressed",
        "sensing_mousedown",
        "sensing_mousex",
        "sensing_mousey",
        "sensing_loudness",
        "sensing_timer",
        "sensing_of",
        "sensing_current",
        "sensing_dayssince2000",
        "sensing_username",
        "sensing_online",
    ]
}

// ---------------------------------------------------------------------------
// Constant folding
// ---------------------------------------------------------------------------

/// A literal known at compile time, in the three shapes Scratch keeps.
#[derive(Clone, Debug, PartialEq)]
enum Const {
    Num(f64),
    Text(String),
    Bool(bool),
}

impl Const {
    fn of(expr: &Expr) -> Option<Const> {
        match expr {
            Expr::Number(raw, _) => raw.parse::<f64>().ok().map(Const::Num),
            Expr::Str(s, _) => Some(Const::Text(s.clone())),
            Expr::Bool(b, _) => Some(Const::Bool(*b)),
            // A folded boolean is already a *block* -- see [`Const::to_expr`] --
            // and `not(not(5))` is the case that needs it read back as a
            // constant: the inner `not` folds to `operator_equals(1, 0)`, and
            // only a pass that can see that as `false` folds the outer one. Left
            // opaque, the fold would stop one level deep.
            Expr::Call(_) => const_condition(expr).map(Const::Bool),
        }
    }

    fn to_expr(&self, pos: Pos) -> Expr {
        match self {
            Const::Num(n) => Expr::Number(format_number(*n), pos),
            Const::Text(s) => Expr::Str(s.clone(), pos),
            // **A boolean is spelled as a block, not as a literal.**
            //
            // Scratch has no boolean *literal*: its hexagonal inputs hold a
            // boolean block and nothing else, so `control_if(true)` is not a
            // thing the emitter can write -- it is exactly the
            // "expected a condition, found `true`" error. A folded boolean
            // therefore has to be replaced by the smallest block that is
            // *always* that boolean, and `operator_equals(1, 1)` / `(1, 0)` are
            // it: both operands are literals, so the VM evaluates the answer
            // without reading anything, and the block is a legal hexagonal
            // input.
            //
            // This is the one place the fold is not a pure win in block count
            // -- a boolean condition that folded to a literal still costs one
            // block -- and it is why the report counts these separately.
            Const::Bool(b) => {
                let (left, right) = if *b { ("1", "1") } else { ("1", "0") };
                Expr::Call(CallExpr {
                    opcode: "operator_equals".to_string(),
                    args: vec![
                        Expr::Number(left.to_string(), pos),
                        Expr::Number(right.to_string(), pos),
                    ],
                    pos,
                    len: "operator_equals".len() as u32,
                })
            }
        }
    }

    /// Scratch's `toNumber`, which is JavaScript's `Number()` on the trimmed
    /// text and zero for anything that is not a number.
    fn number(&self) -> f64 {
        match self {
            Const::Num(n) => *n,
            Const::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Const::Text(s) => {
                let t = s.trim();
                if t.is_empty() {
                    0.0
                } else {
                    t.parse::<f64>().unwrap_or(f64::NAN)
                }
            }
        }
    }

    /// Scratch's `toBoolean`: the strings `"true"`, `"false"` and `""` are
    /// special and everything else is true when it is not the number zero.
    fn boolean(&self) -> bool {
        match self {
            Const::Bool(b) => *b,
            Const::Num(n) => *n != 0.0 && !n.is_nan(),
            Const::Text(s) => match s.to_ascii_lowercase().as_str() {
                "true" => true,
                "false" | "" => false,
                _ => {
                    let n = s.trim().parse::<f64>().unwrap_or(f64::NAN);
                    n != 0.0 && !n.is_nan()
                }
            },
        }
    }

    /// Scratch's string cast, which is JavaScript's `String()` for a number.
    fn text(&self) -> String {
        match self {
            Const::Text(s) => s.clone(),
            Const::Bool(b) => b.to_string(),
            Const::Num(n) => format_number(*n),
        }
    }
}

/// A number as Scratch would spell it, so a folded literal round-trips through
/// `project.json` the way the constant it replaced would have.
fn format_number(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if n.fract() == 0.0 && n.abs() < 1e21 {
        return format!("{}", n as i64);
    }
    let s = format!("{n}");
    s
}

/// Scratch's `%`: floored, not truncated, and `NaN` when the divisor is zero.
fn scratch_mod(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        return f64::NAN;
    }
    let r = a % b;
    if r / b < 0.0 {
        r + b
    } else {
        r
    }
}

/// Scratch's `operator_mathop`. The angles are degrees and `ln`/`log`/`e ^`
/// follow JavaScript's `Math`, which is what the VM calls.
fn scratch_mathop(op: &str, n: f64) -> Option<f64> {
    let v = match op {
        "abs" => n.abs(),
        "floor" => n.floor(),
        "ceiling" => n.ceil(),
        "sqrt" => {
            if n < 0.0 {
                f64::NAN
            } else {
                n.sqrt()
            }
        }
        "sin" => (n * std::f64::consts::PI / 180.0).sin(),
        "cos" => (n * std::f64::consts::PI / 180.0).cos(),
        "tan" => {
            let r = (n * std::f64::consts::PI / 180.0).tan();
            // The VM rounds a tangent that is not finite to Infinity rather
            // than leaving a huge value; copying that keeps the two equal.
            if r.is_nan() || r.is_infinite() {
                f64::NAN
            } else {
                (r * 1e10).round() / 1e10
            }
        }
        "asin" => (n.clamp(-1.0, 1.0)).asin() * 180.0 / std::f64::consts::PI,
        "acos" => (n.clamp(-1.0, 1.0)).acos() * 180.0 / std::f64::consts::PI,
        "atan" => n.atan() * 180.0 / std::f64::consts::PI,
        "ln" => n.ln(),
        "log" => n.log10(),
        "e ^" => n.exp(),
        "10 ^" => n.powf(10.0),
        _ => return None,
    };
    Some(v)
}

/// Scratch's `=`, which is JavaScript's `===` after both sides are cast: two
/// numbers compare numerically, anything else compares as text and is
/// case-insensitive.
fn scratch_equals(a: &Const, b: &Const) -> bool {
    let an = matches!(a, Const::Num(_));
    let bn = matches!(b, Const::Num(_));
    if an && bn {
        return a.number() == b.number();
    }
    let at = a.text().to_lowercase();
    let bt = b.text().to_lowercase();
    if let (Ok(x), Ok(y)) = (at.parse::<f64>(), bt.parse::<f64>()) {
        // A string that reads as a number still compares as a number against
        // another that does, which is Scratch's documented behaviour.
        if at == bt {
            return true;
        }
        return x == y;
    }
    at == bt
}

// ---------------------------------------------------------------------------
// The pass
// ---------------------------------------------------------------------------

/// The block a folded boolean is spelled as, and the literal it means.
///
/// A boolean constant cannot be a literal in this language -- see
/// [`Const::to_expr`] -- so it is spelled as `operator_equals` between two
/// literals, and this is the inverse. Everything that needs to *know* a
/// condition's value rather than emit it goes through here, which keeps the
/// spelling rule in exactly one place.
fn const_condition(expr: &Expr) -> Option<bool> {
    let Expr::Call(call) = expr else {
        return None;
    };
    if call.opcode != "operator_equals" || call.args.len() != 2 {
        return None;
    }
    let left = Const::of(&call.args[0])?;
    let right = Const::of(&call.args[1])?;
    Some(scratch_equals(&left, &right))
}

/// Whether a condition is a literal the optimiser put there.
///
/// `Expr::Bool` is what the *front end* emits for a literal `<true>` in source,
/// and `const_condition` is what a fold leaves. Both are "the compiler knows
/// this answer", which is the only case the `if` splice is allowed to act on: a
/// condition that merely *looks* constant is a different question and this pass
/// does not answer it.
fn literal_condition(expr: &Expr) -> Option<bool> {
    match expr {
        Expr::Bool(b, _) => Some(*b),
        other => const_condition(other),
    }
}

/// What a pass changed, for the report a caller can print.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Folded {
    /// Reporter blocks replaced by a literal.
    pub folded: usize,
    /// An `if` with a literal condition replaced by one branch or by nothing.
    pub simplified: usize,
}

impl Folded {
    pub fn total(&self) -> usize {
        self.folded + self.simplified
    }
}

impl std::fmt::Display for Folded {
    /// A one-line report, because the two rewrites have different effects and a
    /// merged total would hide which one is carrying the pass.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} constant fold(s), {} branch simplification(s)",
            self.folded, self.simplified
        )
    }
}

/// Rewrite one raven-asm file: a target's body if it declares one, its module
/// items otherwise.
///
/// A file is either a target file or a module file and never both, so this is
/// the whole of "optimise a source file" and the entry point [`crate`] uses. It
/// exists because a target keeps its body *inside* the `stage { … }` or `sprite
/// "…" { … }` declaration, so a caller holding a parsed file has one more layer
/// to reach through than [`optimize_target`] knows about.
pub fn optimize_file(unit: &mut File) -> Folded {
    match &mut unit.target {
        Some(target) => optimize_target(&mut target.items),
        None => optimize_target(&mut unit.items),
    }
}

/// Rewrite a target's statements in place, bottom up.
///
/// The order is bottom up because a fold can expose another: `not(not(lt(1,2)))`
/// has to fold the `lt` before the inner `not` can be seen to be a literal, and
/// the inner `not` before the outer.
///
/// A target's own items are *not* treated as a spliceable list. An `if` written
/// at the top level of a target is a hat-less script in its own right, and
/// replacing it with its body would turn one script into a different one. The
/// splice only ever happens inside a body, where it is a rewrite of the same
/// statement chain.
pub fn optimize_target(items: &mut [Item]) -> Folded {
    let mut folded = Folded::default();
    for item in items.iter_mut() {
        optimize_item(item, &mut folded);
    }
    folded
}

/// [`optimize_target`] over a list of items each carrying its own source file.
///
/// The compiler holds its items as `(item, source)` pairs so that a diagnostic
/// can name the file a `use` pulled it from, and the optimiser has no business
/// touching that. This walks the same list and rewrites only the item.
pub fn optimize_scoped<T, F>(entries: &mut [T], item_of: F) -> Folded
where
    F: Fn(&mut T) -> &mut Item,
{
    let mut folded = Folded::default();
    for entry in entries.iter_mut() {
        optimize_item(item_of(entry), &mut folded);
    }
    folded
}

fn optimize_item(item: &mut Item, folded: &mut Folded) {
    match item {
        Item::Proc(decl) => optimize_block(&mut decl.body, folded),
        Item::Stmt(stmt) => optimize_stmt(stmt, folded),
        _ => {}
    }
}

/// Rewrite one statement list, which is where the splices happen.
///
/// A list and not a statement, because two of the rewrites change the *number*
/// of statements: `if <true> { body }` becomes `body` and `if <false> { body }`
/// Whether a statement ends its script and so may not be followed by another.
///
/// This mirrors `Emitter::stmt_is_cap`. It has to be mirrored rather than
/// shared, because the emitter decides it from the *catalog* and the optimiser
/// runs before any catalog lookup -- and getting it wrong is not a cosmetic
/// error: a `control_stop` that ends up with a statement after it is rejected by
/// the emitter with "nothing can follow `control_stop`".
///
/// The two statements that are caps: any block the catalog marks as a cap, and
/// `control_stop` when it has no bottom notch. Only the "other scripts" forms of
/// `control_stop` leave a notch, which is the same rule the emitter applies.
fn stmt_is_cap(stmt: &Stmt) -> bool {
    match catalog::block(&stmt.opcode) {
        Some(spec) if spec.opcode == "control_stop" => !stop_has_next(stmt),
        Some(spec) => spec.kind == catalog::BlockKind::Cap,
        // An unknown opcode is a proc call or an error the emitter will report.
        // Treating it as a cap would suppress rewrites for no reason.
        None => false,
    }
}

fn stop_has_next(stmt: &Stmt) -> bool {
    match stmt.args.first() {
        Some(Expr::Str(value, _)) => matches!(
            value.as_str(),
            "other scripts in sprite" | "other scripts in stage"
        ),
        _ => false,
    }
}

fn optimize_block(stmts: &mut Vec<Stmt>, folded: &mut Folded) {
    for stmt in stmts.iter_mut() {
        optimize_stmt(stmt, folded);
    }

    // The splice, once the statements themselves are rewritten.
    //
    // **A cap block on either side stops the splice**, and both halves of that
    // are load-bearing:
    //
    //   * if the body *ends* in one, the statements after the `if` would become
    //     statements after a cap, which the emitter refuses and which the source
    //     never had -- this is the bug that broke the `rv32-doom` example, where
    //     a folded `if` wrapped an `efb_write_indexed` call ending in
    //     `control_stop("this script")`;
    //   * if the statement *before* the `if` is one, then the `if` is already
    //     unreachable and the source is about to be rejected anyway.
    //
    // Leaving the `if` in place is always correct: it is what the source said.
    let mut out: Vec<Stmt> = Vec::with_capacity(stmts.len());
    let mut after_cap = false;
    for mut stmt in stmts.drain(..) {
        let is_branch = stmt.opcode == "control_if" || stmt.opcode == "control_if_else";
        if !after_cap && is_branch {
            if let Some(value) = stmt.args.first().and_then(literal_condition) {
                // **The branch merge.** With a condition the compiler can see,
                // exactly one of the two branches runs -- `control_if_else`
                // chooses between them on the same test -- so emitting only the
                // taken one is what the block does, and it does it without the
                // condition or the untaken branch. This is the same proof as the
                // no-`else` case and it was refused here for a while on the
                // grounds that "choosing a branch is a different rewrite"; it is
                // not a different rewrite, it is the same one with a second
                // branch to discard.
                //
                // The taken branch is inspected *before* anything is moved out,
                // because the two paths below disagree about whether the
                // statement survives and a `take()` that has already happened
                // cannot be undone.
                let chosen = if value {
                    stmt.body.as_ref()
                } else {
                    stmt.else_body.as_ref()
                };
                // A branch that ends in a cap cannot be spliced into this slot:
                // the statements after the `if` would become statements after a
                // cap, which the emitter refuses.
                let ends_in_cap = chosen.and_then(|b| b.last()).is_some_and(stmt_is_cap);
                if !ends_in_cap {
                    folded.simplified += 1;
                    let body = if value {
                        stmt.body.take()
                    } else {
                        stmt.else_body.take()
                    };
                    if let Some(body) = body {
                        after_cap = body.last().is_some_and(stmt_is_cap);
                        out.extend(body);
                    }
                    continue;
                }
                // The taken branch stays where it is, so the statement survives
                // as an `if` and only the branch that could not have run is
                // dropped. A literal condition is still correct and still legal.
                //
                // When the *else* is the taken one it becomes the body, which is
                // the same statement with its two halves swapped: an `if` whose
                // body is what the `else` was.
                folded.simplified += 1;
                if !value {
                    stmt.body = stmt.else_body.take();
                }
                stmt.else_body = None;
                stmt.opcode = "control_if".to_string();
                out.push(stmt);
                continue;
            }
        }
        if stmt_is_cap(&stmt) {
            after_cap = true;
        }
        out.push(stmt);
    }
    *stmts = out;
}

fn optimize_stmt(stmt: &mut Stmt, folded: &mut Folded) {
    // A body is a place a statement can sit, so it is rewritten first: folding
    // inside a branch can turn the branch into an unconditional one, and that
    // decision has to see the folded condition.
    if let Some(body) = stmt.body.as_mut() {
        optimize_block(body, folded);
    }
    if let Some(else_body) = stmt.else_body.as_mut() {
        optimize_block(else_body, folded);
    }

    for arg in stmt.args.iter_mut() {
        optimize_expr(arg, folded);
    }
}

/// Rewrite an expression, innermost first.
fn optimize_expr(expr: &mut Expr, folded: &mut Folded) {
    let Expr::Call(call) = expr else {
        return;
    };
    for arg in call.args.iter_mut() {
        optimize_expr(arg, folded);
    }

    // Every argument that is a literal is a chance to fold, and every argument
    // that is not means this call cannot be folded at all.
    let consts: Option<Vec<Const>> = call.args.iter().map(Const::of).collect();
    let Some(consts) = consts else {
        return;
    };

    if let Some(replacement) = fold_call(&call.opcode, &consts) {
        *expr = replacement.to_expr(call.pos);
        folded.folded += 1;
    }
}

/// The value a call with all-literal arguments has, or `None` when this pass
/// does not know the answer.
///
/// `None` is the safe answer and it is chosen often: a block whose semantics
/// this pass cannot reproduce *exactly* is left alone, because a fold that
/// disagrees with the VM by one bit is worse than no fold at all.
fn fold_call(opcode: &str, args: &[Const]) -> Option<Const> {
    // `not` and `and`/`or` are boolean coercion, which is a documented two-valued
    // cast in Scratch, so they are identities rather than arithmetic.
    match opcode {
        "operator_not" => {
            let a = args.first()?;
            return Some(Const::Bool(!a.boolean()));
        }
        "operator_and" => {
            let (a, b) = (args.first()?, args.get(1)?);
            return Some(Const::Bool(a.boolean() && b.boolean()));
        }
        "operator_or" => {
            let (a, b) = (args.first()?, args.get(1)?);
            return Some(Const::Bool(a.boolean() || b.boolean()));
        }
        "operator_equals" => {
            let (a, b) = (args.first()?, args.get(1)?);
            return Some(Const::Bool(scratch_equals(a, b)));
        }
        "operator_lt" => {
            let (a, b) = (args.first()?, args.get(1)?);
            return Some(Const::Bool(compare(a, b)? < 0));
        }
        "operator_gt" => {
            let (a, b) = (args.first()?, args.get(1)?);
            return Some(Const::Bool(compare(a, b)? > 0));
        }
        _ => {}
    }

    // Arithmetic. Every one of these is JavaScript's own operator on `Number()`,
    // which is what the VM evaluates, so the answers agree bit for bit.
    let a = args.first()?.number();
    match opcode {
        "operator_add" => Some(Const::Num(a + args.get(1)?.number())),
        "operator_subtract" => Some(Const::Num(a - args.get(1)?.number())),
        "operator_multiply" => Some(Const::Num(a * args.get(1)?.number())),
        "operator_divide" => Some(Const::Num(a / args.get(1)?.number())),
        "operator_mod" => Some(Const::Num(scratch_mod(a, args.get(1)?.number()))),
        "operator_round" => Some(Const::Num(round_half_up(a))),
        "operator_mathop" => {
            let op = match args.first()? {
                // The operator is a menu, so it arrives as a string.
                Const::Text(s) => s.clone(),
                _ => return None,
            };
            // `args[0]` is the menu; the operand is `args[1]`.
            let operand = args.get(1)?.number();
            scratch_mathop(&op, operand).map(Const::Num)
        }
        _ => None,
    }
}

/// JavaScript's `Math.round`, which Scratch's `round` calls: half rounds up,
/// and it is *not* Rust's `round` for negative halves.
fn round_half_up(n: f64) -> f64 {
    (n + 0.5).floor()
}

/// Scratch's `<` and `>`, which compare numerically when both sides read as
/// numbers and by text (case-insensitively) when they do not.
fn compare(a: &Const, b: &Const) -> Option<i32> {
    let an = a.number();
    let bn = b.number();
    if !an.is_nan() && !bn.is_nan() {
        return an.partial_cmp(&bn).map(|o| o as i32);
    }
    let at = a.text().to_lowercase();
    let bt = b.text().to_lowercase();
    Some(match at.cmp(&bt) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
//
// The obligation this pass has is not "it folds things" but "it never changes
// what the project does". So the tests below are in three groups: the folds that
// must happen, the folds that must *not* happen, and the arithmetic, which is
// checked against the values Scratch's own VM produces for the same expressions.
//
// The last group is the important one, and it is written from `scratch3_operators.js`
// rather than from intuition: `mod` is floored, `round` is JavaScript's, `=` is
// `===` after a cast, and a string that reads as a number compares as a number.
// A fold that disagrees with any of those by one bit is a wrong program, so each
// case names the VM rule it follows.

#[cfg(test)]
mod tests {
    use super::*;

    // -- helpers ----------------------------------------------------------

    fn num(n: f64) -> Expr {
        Expr::Number(format_number(n), Pos::new(1, 1))
    }
    fn text(s: &str) -> Expr {
        Expr::Str(s.to_string(), Pos::new(1, 1))
    }
    fn boolean(b: bool) -> Expr {
        Expr::Bool(b, Pos::new(1, 1))
    }
    fn call(opcode: &str, args: Vec<Expr>) -> Expr {
        Expr::Call(CallExpr {
            opcode: opcode.to_string(),
            args,
            pos: Pos::new(1, 1),
            len: opcode.len() as u32,
        })
    }
    fn stmt(opcode: &str, args: Vec<Expr>) -> Stmt {
        Stmt {
            opcode: opcode.to_string(),
            args,
            body: None,
            else_body: None,
            pos: Pos::new(1, 1),
            len: opcode.len() as u32,
        }
    }

    /// Run the pass over one expression and hand back what it became.
    ///
    /// The expression is put in the value slot of a `set` so it is reachable,
    /// and the slot is read back afterwards. Nothing about the wrapper can be
    /// folded -- `data_setvariableto` is a command -- so what comes back is what
    /// the pass made of the expression and nothing else.
    fn folded_expr(expr: Expr) -> Expr {
        let mut items = vec![Item::Stmt(stmt(
            "data_setvariableto",
            vec![text("x"), expr],
        ))];
        optimize_target(&mut items);
        let Item::Stmt(mut s) = items.remove(0) else {
            panic!("expected a statement")
        };
        s.args.remove(1)
    }

    /// An expression as text, for comparing two trees.
    ///
    /// The AST deliberately has no `PartialEq` -- two subtrees are equal when
    /// they *are* the same spelling, and deriving it would invite comparing
    /// trees that differ only in a `Pos`. Spelling them is the comparison these
    /// tests want anyway: it is what a reader would check, and it is what a
    /// failure message can show.
    fn spelled(expr: &Expr) -> String {
        match expr {
            Expr::Number(raw, _) => raw.clone(),
            Expr::Str(s, _) => format!("\"{s}\""),
            Expr::Bool(b, _) => b.to_string(),
            Expr::Call(c) => format!(
                "{}({})",
                c.opcode,
                c.args.iter().map(spelled).collect::<Vec<_>>().join(", ")
            ),
        }
    }

    /// The fold of one call, as text. Every assertion goes through this, so a
    /// failure says what it got rather than only that it differed.
    fn fold_of(opcode: &str, args: Vec<Expr>) -> String {
        spelled(&folded_expr(call(opcode, args)))
    }

    /// A folded boolean, as the `true`/`false` a reader means.
    ///
    /// A folded condition is *spelled* as `operator_equals` between two literals
    /// -- see [`Const::to_expr`] -- so comparing it to the string `"true"` would
    /// be asserting the spelling rather than the value. This reads the value,
    /// and the spelling is pinned separately by
    /// `a_folded_boolean_is_a_block_and_never_a_bare_literal`.
    fn fold_bool(opcode: &str, args: Vec<Expr>) -> String {
        let folded = folded_expr(call(opcode, args));
        match literal_condition(&folded) {
            Some(b) => b.to_string(),
            None => panic!("not a folded condition: {}", spelled(&folded)),
        }
    }

    /// The numeric value of a folded call, for the cases where the spelling is
    /// not the point.
    fn folded_number(opcode: &str, args: Vec<Expr>) -> f64 {
        match folded_expr(call(opcode, args)) {
            Expr::Number(raw, _) => raw.parse::<f64>().unwrap_or_else(|_| panic!("`{raw}`")),
            other => panic!("expected a number, got {}", spelled(&other)),
        }
    }

    /// Run the pass over one body and hand back the statements it left.
    fn folded_body(stmts: Vec<Stmt>) -> Vec<Stmt> {
        let mut items = vec![Item::Stmt(Stmt {
            opcode: "control_repeat".to_string(),
            args: vec![num(1.0)],
            body: Some(stmts),
            else_body: None,
            pos: Pos::new(1, 1),
            len: 0,
        })];
        optimize_target(&mut items);
        let Item::Stmt(s) = items.remove(0) else {
            panic!("expected a statement")
        };
        s.body.unwrap_or_default()
    }

    // -- the folds that must happen ---------------------------------------

    #[test]
    fn integer_arithmetic_folds() {
        assert_eq!(fold_of("operator_add", vec![num(2.0), num(3.0)]), "5");
        assert_eq!(fold_of("operator_subtract", vec![num(2.0), num(3.0)]), "-1");
        assert_eq!(fold_of("operator_multiply", vec![num(6.0), num(7.0)]), "42");
        assert_eq!(fold_of("operator_divide", vec![num(1.0), num(4.0)]), "0.25");
    }

    #[test]
    fn a_folded_boolean_is_a_block_and_never_a_bare_literal() {
        // **The bug this test exists for.** Scratch has no boolean literal: a
        // hexagonal input holds a boolean *block*. Folding `lt(1,2)` to a bare
        // `Expr::Bool(true)` produced a tree the emitter refuses with
        // "expected a condition, found `true`", and it broke two of this
        // repository's own examples -- `penfont` and `chess` -- the first time
        // the optimiser ran over them. So a folded boolean has to come back as
        // a block, and this pins the spelling.
        let folded = folded_expr(call("operator_lt", vec![num(1.0), num(2.0)]));
        assert_eq!(spelled(&folded), "operator_equals(1, 1)");
        assert_eq!(const_condition(&folded), Some(true));

        let folded = folded_expr(call("operator_lt", vec![num(2.0), num(1.0)]));
        assert_eq!(spelled(&folded), "operator_equals(1, 0)");
        assert_eq!(const_condition(&folded), Some(false));

        // And the spelling really is *true* and *false* in Scratch's own
        // semantics, which is what makes it a legal stand-in.
        assert!(scratch_equals(&Const::Num(1.0), &Const::Num(1.0)));
        assert!(!scratch_equals(&Const::Num(1.0), &Const::Num(0.0)));
    }

    #[test]
    fn nested_folds_expose_outer_ones() {
        // `(1 + 2) * 3`, whose inner add has to fold before the multiply can.
        let inner = call("operator_add", vec![num(1.0), num(2.0)]);
        assert_eq!(fold_of("operator_multiply", vec![inner, num(3.0)]), "9");
    }

    #[test]
    fn modulo_is_floored_like_the_vm() {
        // `scratch3_operators.js` does `let r = n % m; if (r / m < 0) r += m;`
        // JavaScript's `%` truncates, so a negative result needs the fixup and a
        // zero divisor is `NaN` rather than an error.
        assert_eq!(fold_of("operator_mod", vec![num(-7.0), num(3.0)]), "2");
        assert_eq!(fold_of("operator_mod", vec![num(7.0), num(3.0)]), "1");
        assert_eq!(fold_of("operator_mod", vec![num(-6.0), num(3.0)]), "0");
        assert_eq!(fold_of("operator_mod", vec![num(1.0), num(0.0)]), "NaN");
    }

    #[test]
    fn round_is_javascripts_and_not_rusts() {
        // Rust's `f64::round` rounds half away from zero; JavaScript's
        // `Math.round` rounds half up, so `-2.5` is `-2` there and `-3` here.
        assert_eq!(fold_of("operator_round", vec![num(2.5)]), "3");
        assert_eq!(fold_of("operator_round", vec![num(-2.5)]), "-2");
        assert_eq!(fold_of("operator_round", vec![num(2.4)]), "2");
    }

    #[test]
    fn boolean_identities_fold() {
        assert_eq!(fold_bool("operator_not", vec![boolean(true)]), "false");
        assert_eq!(fold_bool("operator_not", vec![boolean(false)]), "true");
        assert_eq!(
            fold_bool("operator_and", vec![boolean(true), boolean(false)]),
            "false"
        );
        assert_eq!(
            fold_bool("operator_or", vec![boolean(false), boolean(true)]),
            "true"
        );
    }

    #[test]
    fn not_not_collapses_to_a_boolean_and_not_to_the_operand() {
        // The tempting rewrite `not(not(x)) -> x` is **wrong**: Scratch's
        // `toBoolean` is not the identity on a number, so `not(not(5))` is
        // `true` rather than `5`. What is sound is folding the inner `not` to a
        // literal and then the outer, and this test pins that.
        let inner = call("operator_not", vec![num(5.0)]);
        assert_eq!(fold_bool("operator_not", vec![inner]), "true");
    }

    #[test]
    fn equality_follows_the_vms_casting_rule() {
        assert_eq!(
            fold_bool("operator_equals", vec![num(1.0), num(1.0)]),
            "true"
        );
        // Text compares case-insensitively.
        assert_eq!(
            fold_bool("operator_equals", vec![text("AbC"), text("abc")]),
            "true"
        );
        // A string that reads as a number equals that number.
        assert_eq!(
            fold_bool("operator_equals", vec![text("1"), num(1.0)]),
            "true"
        );
        assert_eq!(
            fold_bool("operator_equals", vec![text("1"), num(2.0)]),
            "false"
        );
    }

    #[test]
    fn comparison_follows_the_vms_casting_rule() {
        assert_eq!(fold_bool("operator_lt", vec![num(1.0), num(2.0)]), "true");
        assert_eq!(fold_bool("operator_gt", vec![num(1.0), num(2.0)]), "false");
        assert_eq!(fold_bool("operator_lt", vec![text("a"), text("b")]), "true");
    }

    #[test]
    fn if_true_is_replaced_by_its_body() {
        let mut cond = stmt("control_if", vec![boolean(true)]);
        cond.body = Some(vec![stmt("looks_say", vec![text("a")])]);
        let out = folded_body(vec![cond, stmt("looks_say", vec![text("b")])]);
        assert_eq!(out.len(), 2, "`if true` should splice its body in place");
        assert!(out.iter().all(|s| s.opcode == "looks_say"));
    }

    #[test]
    fn if_false_and_its_body_are_dropped() {
        let mut cond = stmt("control_if", vec![boolean(false)]);
        cond.body = Some(vec![stmt("looks_say", vec![text("a")])]);
        assert!(
            folded_body(vec![cond]).is_empty(),
            "`if false` should emit nothing"
        );
    }

    #[test]
    fn a_body_ending_in_a_cap_block_is_not_spliced() {
        // **The bug this test exists for.** A cap block ends its script and may
        // not be followed by another statement. Splicing `if true { … }` whose
        // body ends in one moves the statements *after* the `if` to after the
        // cap, which the emitter refuses with "nothing can follow
        // `control_stop`". It broke the `rv32-doom` example, where a folded `if`
        // wrapped a call whose body ends in `control_stop("this script")`.
        //
        // Leaving the `if` alone is always correct, so that is what the pass
        // does: the block stays, and the fold that produced its condition is
        // kept.
        let mut cond = stmt("control_if", vec![boolean(true)]);
        cond.body = Some(vec![
            stmt("looks_say", vec![text("a")]),
            stmt("control_stop", vec![text("this script")]),
        ]);
        let out = folded_body(vec![cond, stmt("looks_say", vec![text("b")])]);
        assert_eq!(out.len(), 2, "the `if` must survive: {:?}", out.len());
        assert_eq!(out[0].opcode, "control_if");
        assert_eq!(out[1].opcode, "looks_say");
    }

    #[test]
    fn a_notched_stop_is_not_a_cap() {
        // `stop other scripts in sprite` leaves a bottom notch, so a statement
        // may follow it and a body ending in one *is* spliceable. The guard has
        // to tell the two forms apart or it would refuse a correct rewrite.
        assert!(!stmt_is_cap(&stmt(
            "control_stop",
            vec![text("other scripts in sprite")]
        )));
        assert!(stmt_is_cap(&stmt(
            "control_stop",
            vec![text("this script")]
        )));
        assert!(stmt_is_cap(&stmt("control_delete_this_clone", vec![])));
    }

    #[test]
    fn a_statement_after_a_cap_is_left_alone() {
        // If a cap already precedes the `if`, the source is about to be rejected
        // by the emitter and the optimiser must not make it worse by moving
        // statements around.
        let cap = stmt("control_stop", vec![text("this script")]);
        let mut cond = stmt("control_if", vec![boolean(true)]);
        cond.body = Some(vec![stmt("looks_say", vec![text("a")])]);
        let out = folded_body(vec![cap, cond]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].opcode, "control_stop");
        assert_eq!(out[1].opcode, "control_if");
    }

    #[test]
    fn a_fold_inside_a_condition_can_expose_an_if() {
        // `if <not(false)>` is not a literal until the inner `not` folds, which
        // is the whole reason the pass runs bottom up.
        let mut cond = stmt(
            "control_if",
            vec![call("operator_not", vec![boolean(false)])],
        );
        cond.body = Some(vec![stmt("looks_say", vec![text("a")])]);
        let out = folded_body(vec![cond]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].opcode, "looks_say");
    }

    #[test]
    fn a_condition_folded_from_reporters_also_splices() {
        // The two halves meeting: `lt(1, 2)` folds to a `Const`, becomes
        // `Expr::Bool`, and the splice then sees a literal condition.
        let mut cond = stmt(
            "control_if",
            vec![call("operator_lt", vec![num(1.0), num(2.0)])],
        );
        cond.body = Some(vec![stmt("looks_say", vec![text("a")])]);
        let out = folded_body(vec![cond]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].opcode, "looks_say");
    }

    // -- the rewrites that must not happen --------------------------------

    #[test]
    fn a_variable_is_never_propagated() {
        // `data_variable` may be assigned between two reads, so nothing here may
        // replace it -- not even when the script just set it.
        let read = call("data_variable", vec![text("x")]);
        let before = spelled(&read);
        assert_eq!(spelled(&folded_expr(read)), before);
    }

    #[test]
    fn a_sampled_reporter_is_never_folded() {
        for opcode in [
            "sensing_timer",
            "sensing_answer",
            "sensing_mousex",
            "looks_size",
        ] {
            assert!(!is_pure(opcode), "`{opcode}` must not be pure");
            let e = call(opcode, vec![]);
            let before = spelled(&e);
            assert_eq!(spelled(&folded_expr(e)), before, "`{opcode}` was rewritten");
        }
    }

    #[test]
    fn an_expression_with_a_non_literal_argument_is_left_alone() {
        let unknown = call("data_variable", vec![text("x")]);
        let e = call("operator_add", vec![num(2.0), unknown]);
        let before = spelled(&e);
        assert_eq!(spelled(&folded_expr(e)), before);
    }

    #[test]
    fn if_else_with_a_true_condition_keeps_the_then_branch() {
        // `control_if_else` chooses between its two branches on the same test,
        // so a condition the compiler can see means exactly one of them runs.
        // Emitting that one alone is what the block does, minus the branch that
        // could not have run.
        let mut cond = stmt("control_if_else", vec![boolean(true)]);
        cond.body = Some(vec![stmt("looks_say", vec![text("then")])]);
        cond.else_body = Some(vec![stmt("looks_say", vec![text("else")])]);
        let out = folded_body(vec![cond]);
        assert_eq!(out.len(), 1, "only the taken branch should remain");
        assert_eq!(out[0].opcode, "looks_say");
        // And it is the *taken* one, not the other.
        assert_eq!(spelled(&out[0].args[0]), "\"then\"");
    }

    #[test]
    fn if_else_with_a_false_condition_keeps_the_else_branch() {
        let mut cond = stmt("control_if_else", vec![boolean(false)]);
        cond.body = Some(vec![stmt("looks_say", vec![text("then")])]);
        cond.else_body = Some(vec![stmt("looks_say", vec![text("else")])]);
        let out = folded_body(vec![cond]);
        assert_eq!(out.len(), 1, "only the taken branch should remain");
        assert_eq!(spelled(&out[0].args[0]), "\"else\"");
    }

    #[test]
    fn if_else_with_a_folded_condition_merges_too() {
        // The condition is not a literal until the fold runs, which is why the
        // merge happens in the same bottom-up pass as the folding.
        let mut cond = stmt(
            "control_if_else",
            vec![call("operator_lt", vec![num(1.0), num(2.0)])],
        );
        cond.body = Some(vec![stmt("looks_say", vec![text("then")])]);
        cond.else_body = Some(vec![stmt("looks_say", vec![text("else")])]);
        let out = folded_body(vec![cond]);
        assert_eq!(out.len(), 1);
        assert_eq!(spelled(&out[0].args[0]), "\"then\"");
    }

    #[test]
    fn if_else_with_a_runtime_condition_is_left_alone() {
        // The condition is a variable, so it is not the compiler's to know.
        // This is the guard that keeps the merge from changing a real branch.
        let cond_expr = call(
            "operator_lt",
            vec![call("data_variable", vec![text("x")]), num(2.0)],
        );
        let mut cond = stmt("control_if_else", vec![cond_expr]);
        cond.body = Some(vec![stmt("looks_say", vec![text("then")])]);
        cond.else_body = Some(vec![stmt("looks_say", vec![text("else")])]);
        let out = folded_body(vec![cond]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].opcode, "control_if_else");
        assert!(out[0].else_body.is_some(), "both branches must survive");
    }

    #[test]
    fn an_if_else_whose_taken_branch_ends_in_a_cap_keeps_both_branches() {
        // Splicing would put statements after a cap, but the *untaken* branch is
        // still dead and is dropped: the statement becomes an `if` with a
        // literal condition, which is correct and legal.
        let mut cond = stmt("control_if_else", vec![boolean(true)]);
        cond.body = Some(vec![
            stmt("looks_say", vec![text("then")]),
            stmt("control_stop", vec![text("this script")]),
        ]);
        cond.else_body = Some(vec![stmt("looks_say", vec![text("else")])]);
        let out = folded_body(vec![cond]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].opcode, "control_if", "the else should be dropped");
        assert!(out[0].else_body.is_none());
        assert_eq!(out[0].body.as_ref().map(Vec::len), Some(2));
    }

    #[test]
    fn a_command_is_not_a_value_and_a_hat_is_not_touched() {
        assert!(is_pure("operator_add"));
        assert!(is_pure("operator_lt"));
        assert!(!is_pure("motion_movesteps"), "a command is not a value");
        assert!(!is_pure("event_whenflagclicked"), "a hat is not a value");
        assert!(
            !is_pure("not_a_real_opcode"),
            "an unknown opcode must be treated as impure"
        );
    }

    // -- the maths, against the VM's own definitions -----------------------

    #[test]
    fn mathop_matches_the_vms_table() {
        let at = |op: &str, n: f64| vec![text(op), num(n)];
        assert_eq!(folded_number("operator_mathop", at("abs", -3.0)), 3.0);
        assert_eq!(folded_number("operator_mathop", at("floor", -1.5)), -2.0);
        assert_eq!(folded_number("operator_mathop", at("ceiling", -1.5)), -1.0);
        assert_eq!(folded_number("operator_mathop", at("sqrt", 16.0)), 4.0);
        assert!((folded_number("operator_mathop", at("sin", 90.0)) - 1.0).abs() < 1e-12);
        assert!((folded_number("operator_mathop", at("atan", 1.0)) - 45.0).abs() < 1e-12);
        assert_eq!(folded_number("operator_mathop", at("log", 100.0)), 2.0);
        // A negative square root is `NaN` in the VM rather than an error.
        assert!(folded_number("operator_mathop", at("sqrt", -1.0)).is_nan());
    }

    #[test]
    fn a_fold_produces_a_number_that_round_trips_through_json() {
        // The literal is written into `project.json` as JSON, so what this
        // produces has to parse back to the same value.
        for n in [0.0, -0.0, 1.0, -1.0, 0.5, 1e15, -1e15, 0.1] {
            let written = format_number(n);
            let back: f64 = written
                .parse()
                .unwrap_or_else(|_| panic!("`{written}` is not a number"));
            assert_eq!(back, n, "`{written}` did not round-trip");
        }
    }

    #[test]
    fn the_pass_reports_what_it_did() {
        let mut items = vec![Item::Stmt(stmt(
            "data_setvariableto",
            vec![text("x"), call("operator_add", vec![num(1.0), num(1.0)])],
        ))];
        let folded = optimize_target(&mut items);
        assert_eq!(folded.folded, 1);
        assert_eq!(folded.total(), 1);
    }

    #[test]
    fn the_pass_is_idempotent() {
        // A second run over an already-folded tree finds nothing to do. That is
        // what makes it safe to apply to a target the front end has already
        // lowered once, and it is the property `BuildOptions::default()` leans
        // on: with the flag off, the pass simply does not run.
        let source = || {
            vec![Item::Stmt(stmt(
                "data_setvariableto",
                vec![text("x"), call("operator_add", vec![num(1.0), num(1.0)])],
            ))]
        };
        let mut once = source();
        optimize_target(&mut once);
        let mut twice = once.clone();
        let folded = optimize_target(&mut twice);
        assert_eq!(folded.total(), 0, "the pass is not idempotent");
        assert_eq!(format!("{once:?}"), format!("{twice:?}"));
    }
}
