//! The surface syntax of raven, exactly as the parser produces it.
//!
//! Two things about this tree are worth knowing before reading it.
//!
//! **It is small on purpose.** raven's *core* is Scratch's grammar: an expression
//! is a block, a statement is a block, and there is nothing that is both. The
//! fixed-shape conveniences that are pure substitution — `while c { … }` becomes
//! `while_loop(c, { … })` and `for i in a..b { … }` becomes
//! `for_range(i, a, b, { … })` — are represented here as a [`MacroCall`], because
//! that is literally what they are. `x += e` is not, because its lowering depends
//! on where `x` lives, so it is the core node [`Stmt::CompoundAssign`].
//!
//! **It carries spans, not lines.** Every node has a [`Span`], so an error inside
//! a macro expansion can still point at the token in the call that produced it.
//!
//! The two forms that need repetition rather than substitution — `f"…"` with its
//! holes, and `match` with its arms — are core nodes ([`Expr::Interpolated`] and
//! [`Stmt::Match`]) with a one-line lowering rule each, rather than macros.

use std::path::PathBuf;

use crate::diag::Span;
use crate::ty::{Scalar, Ty};

/// An identifier together with where it was written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

impl Ident {
    #[must_use]
    pub fn new(name: impl Into<String>, span: Span) -> Self {
        Self {
            name: name.into(),
            span,
        }
    }
}

/// A `::`-separated path: `score`, `motion::move_steps`, `lib::geometry::area`,
/// `Key::Space`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Path {
    pub segments: Vec<Ident>,
    pub span: Span,
}

impl Path {
    #[must_use]
    pub fn single(ident: Ident) -> Self {
        Self {
            span: ident.span,
            segments: vec![ident],
        }
    }

    /// The last segment, which is the item being named.
    #[must_use]
    pub fn last(&self) -> &Ident {
        self.segments
            .last()
            .expect("a path always has at least one segment")
    }

    /// Everything before the last segment.
    #[must_use]
    pub fn prefix(&self) -> &[Ident] {
        self.segments.split_last().map_or(&[], |(_, head)| head)
    }

    /// The path as written, without spaces.
    #[must_use]
    pub fn display(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join("::")
    }

    /// Whether the path is a single name.
    #[must_use]
    pub fn is_single(&self) -> bool {
        self.segments.len() == 1
    }
}

// ---------------------------------------------------------------------------
// Files and items
// ---------------------------------------------------------------------------

/// One parsed `.rav` file.
#[derive(Clone, Debug)]
pub struct File {
    pub path: PathBuf,
    pub uses: Vec<UseDecl>,
    pub items: Vec<Item>,
    pub span: Span,
}

impl File {
    /// The target the file declares, if it declares one.
    #[must_use]
    pub fn target(&self) -> Option<&TargetDecl> {
        self.items.iter().find_map(|item| match item {
            Item::Target(target) => Some(target),
            _ => None,
        })
    }
}

/// `use lib::geometry::{hypot, square};`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UseDecl {
    pub path: Path,
    /// `None` when the whole module is imported, `Some` for a braced list.
    pub names: Option<Vec<Ident>>,
    pub span: Span,
}

/// A top-level item.
#[derive(Clone, Debug)]
pub enum Item {
    Target(TargetDecl),
    Var(VarDecl),
    Const(ConstDecl),
    /// `struct Point { x: num, y: num }` — a named frame in the VMS.
    Struct(StructDecl),
    /// `watch score;` — put this state on the stage, as a monitored Scratch
    /// variable that follows the VMS cell.
    Watch(WatchDecl),
    Broadcast(BroadcastDecl),
    Costume(CostumeDecl),
    Sound(SoundDecl),
    Proc(ProcDecl),
    Fn(FnDecl),
    Macro(MacroDecl),
    Script(ScriptDecl),
}

impl Item {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Item::Target(item) => item.span,
            Item::Var(item) => item.span,
            Item::Const(item) => item.span,
            Item::Struct(item) => item.span,
            Item::Watch(item) => item.span,
            Item::Broadcast(item) => item.span,
            Item::Costume(item) => item.span,
            Item::Sound(item) => item.span,
            Item::Proc(item) => item.span,
            Item::Fn(item) => item.span,
            Item::Macro(item) => item.span,
            Item::Script(item) => item.span,
        }
    }

    /// Whether the item may be imported by another module.
    #[must_use]
    pub fn is_public(&self) -> bool {
        match self {
            Item::Var(item) => item.public,
            Item::Const(item) => item.public,
            Item::Struct(item) => item.public,
            // A watch is a configuration, not an item another file imports.
            Item::Watch(_) => false,
            Item::Proc(item) => item.public,
            Item::Fn(item) => item.public,
            Item::Macro(item) => item.public,
            // A target is the file; broadcasts, costumes and sounds belong to a
            // target; a script is never importable.
            _ => false,
        }
    }

    /// The name the item introduces, when it introduces one.
    #[must_use]
    pub fn name(&self) -> Option<&Ident> {
        match self {
            Item::Var(item) => Some(&item.name),
            Item::Const(item) => Some(&item.name),
            Item::Struct(item) => Some(&item.name),
            Item::Watch(_) => None,
            Item::Proc(item) => Some(&item.name),
            Item::Fn(item) => Some(&item.name),
            Item::Macro(item) => Some(&item.name),
            _ => None,
        }
    }

    /// What the item is, for diagnostics.
    #[must_use]
    pub fn describe(&self) -> &'static str {
        match self {
            Item::Target(_) => "target",
            Item::Var(_) => "variable",
            Item::Const(_) => "constant",
            Item::Struct(_) => "struct",
            Item::Watch(_) => "watch",
            Item::Broadcast(_) => "broadcast",
            Item::Costume(_) => "costume",
            Item::Sound(_) => "sound",
            Item::Proc(_) => "procedure",
            Item::Fn(_) => "function",
            Item::Macro(_) => "macro",
            Item::Script(_) => "script",
        }
    }
}

/// `stage { … }` or `sprite "Player" { … }`.
#[derive(Clone, Debug)]
pub struct TargetDecl {
    pub kind: TargetKind,
    /// The stage is always `Stage`; a sprite carries the name it was given.
    pub name: String,
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetKind {
    Stage,
    Sprite,
}

/// `var score: num = 0;` or `pub var trail: list<num> = [];`
#[derive(Clone, Debug)]
pub struct VarDecl {
    pub public: bool,
    pub name: Ident,
    pub ty: Ty,
    pub init: Initializer,
    pub span: Span,
}

/// What a declaration starts life as. raven does not compute these, because the
/// editor does not: a computed starting value belongs in a script. The one
/// exception is a struct, which is built by naming its fields.
#[derive(Clone, Debug)]
pub enum Initializer {
    Value(Literal),
    Items(Vec<Literal>),
    /// `Point { x: 0, y: 0 }` — a struct's fields, in the order written.
    Fields(Vec<(Ident, Expr)>),
}

impl VarDecl {
    /// Whether the declaration is a list written `[]`.
    #[must_use]
    pub fn init_items_empty(&self) -> bool {
        matches!(&self.init, Initializer::Items(items) if items.is_empty())
    }
}

/// `struct Point { x: num, y: num }`
///
/// A struct is a **place**: a fixed run of VMS cells whose field offsets the
/// compiler decides. It is not a value, so it cannot be copied, compared, or
/// stored in one cell — which is what makes a struct field a constant index
/// rather than a lookup, and why there is no ownership question to answer.
#[derive(Clone, Debug)]
pub struct StructDecl {
    pub public: bool,
    pub name: Ident,
    pub fields: Vec<StructField>,
    pub span: Span,
}

/// `watch score, best;`
///
/// A raven `var` is a cell of the virtual memory system, so the editor has
/// nothing to show for it. Naming it here declares a **monitor hook**: a real
/// Scratch variable of the same name, whose monitor starts visible, and which
/// every write to the cell keeps in step. It is the one place raven declares a
/// Scratch variable, and it exists to be looked at, not to be programmed with.
#[derive(Clone, Debug)]
pub struct WatchDecl {
    pub names: Vec<Ident>,
    pub span: Span,
}

/// One field of a struct.
#[derive(Clone, Debug)]
pub struct StructField {
    pub name: Ident,
    pub ty: Ty,
}

/// `const MAX_SPEED: num = 12;`
#[derive(Clone, Debug)]
pub struct ConstDecl {
    pub public: bool,
    pub name: Ident,
    pub ty: Ty,
    pub value: Literal,
    pub span: Span,
}

/// `broadcast "reset";`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BroadcastDecl {
    pub name: String,
    pub span: Span,
}

/// `costume "idle" = "assets/idle.svg" center 32 32;`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CostumeDecl {
    pub name: String,
    pub path: String,
    pub center: Option<(String, String)>,
    pub span: Span,
    pub path_span: Span,
}

/// `sound "beep" = "assets/beep.wav";`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundDecl {
    pub name: String,
    pub path: String,
    pub span: Span,
    pub path_span: Span,
}

/// A literal, with the exact spelling the source used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Literal {
    pub kind: LiteralKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiteralKind {
    /// Kept as text so `1.50` stays `1.50` in the project.
    Number(String),
    Str(String),
    Bool(bool),
}

impl Literal {
    #[must_use]
    pub fn ty(&self) -> Ty {
        match self.kind {
            LiteralKind::Number(_) => Ty::Num,
            LiteralKind::Str(_) => Ty::Str,
            LiteralKind::Bool(_) => Ty::Bool,
        }
    }

    #[must_use]
    pub fn describe(&self) -> String {
        match &self.kind {
            LiteralKind::Number(text) => format!("the number `{text}`"),
            LiteralKind::Str(text) => format!("the string \"{text}\""),
            LiteralKind::Bool(value) => format!("`{value}`"),
        }
    }
}

// ---------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------

/// `proc zigzag(degrees: num, steps: num) -> num warp { … }`
#[derive(Clone, Debug)]
pub struct ProcDecl {
    pub public: bool,
    pub name: Ident,
    pub params: Vec<Param>,
    /// `Some` when the procedure declares `-> ty` and may be called in
    /// expression position.
    pub ret: Option<Ty>,
    pub warp: bool,
    pub body: Block,
    pub span: Span,
}

/// A `proc` parameter: a Scratch custom-block argument, with its reporter kind
/// decided by the declared type.
#[derive(Clone, Debug)]
pub struct Param {
    pub name: Ident,
    pub ty: Scalar,
    pub span: Span,
}

/// `fn hypot(a: num, b: num) -> num { … }`
///
/// An `fn` is a macro with an expression body; see [`MacroDecl`].
#[derive(Clone, Debug)]
pub struct FnDecl {
    pub public: bool,
    pub name: Ident,
    pub params: Vec<TypedParam>,
    pub ret: Ty,
    pub body: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TypedParam {
    pub name: Ident,
    pub ty: Ty,
    pub span: Span,
}

/// `macro count_up($times: expr<num>, $body: block) -> stmts { … }`
#[derive(Clone, Debug)]
pub struct MacroDecl {
    pub public: bool,
    pub name: Ident,
    pub params: Vec<MacroParam>,
    pub result: MacroResult,
    pub body: MacroBody,
    pub span: Span,
}

/// A macro's body: one expression when it produces a value, statements when it
/// does not.
#[derive(Clone, Debug)]
pub enum MacroBody {
    Expr(Expr),
    Stmts(Block),
}

impl MacroBody {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            MacroBody::Expr(expr) => expr.span(),
            MacroBody::Stmts(block) => block.first().map_or_else(Span::default, Stmt::span),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MacroParam {
    /// Written with a `$` in the source; the `$` is not part of the name.
    pub name: Ident,
    pub kind: MacroParamKind,
    pub span: Span,
}

/// What a macro parameter accepts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacroParamKind {
    /// An expression, optionally required to have a type.
    Expr(Option<Ty>),
    /// An identifier, substituted at the call site rather than renamed.
    Ident,
    /// A `{ … }` statement block.
    Block,
}

impl MacroParamKind {
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            MacroParamKind::Expr(None) => "expr".to_string(),
            MacroParamKind::Expr(Some(ty)) => format!("expr<{}>", ty.name()),
            MacroParamKind::Ident => "ident".to_string(),
            MacroParamKind::Block => "block".to_string(),
        }
    }
}

/// What a macro produces, and therefore where it may be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroResult {
    Expr(Ty),
    Stmts,
}

// ---------------------------------------------------------------------------
// Scripts
// ---------------------------------------------------------------------------

/// `on flag_clicked { … }`
#[derive(Clone, Debug)]
pub struct ScriptDecl {
    pub hat: Hat,
    pub body: Block,
    pub span: Span,
}

/// A hat block, named by the catalog with the `event_` prefix dropped.
#[derive(Clone, Debug)]
pub struct Hat {
    pub name: Ident,
    pub args: Vec<Expr>,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

/// A brace-delimited sequence of statements.
pub type Block = Vec<Stmt>;

/// A statement.
///
/// [`Stmt::If`], [`Stmt::Loop`], [`Stmt::Match`] and [`Stmt::CompoundAssign`] are
/// core: they are statements in Scratch and therefore statements here. `while`
/// and `for` are not — they arrive as [`Stmt::Macro`], desugared by the parser.
#[derive(Clone, Debug)]
pub enum Stmt {
    Let(LetStmt),
    Assign(AssignStmt),
    /// `x += e;` — a core form, because a Scratch variable and a VMS cell change
    /// by different blocks.
    CompoundAssign(CompoundAssignStmt),
    /// `return e;` inside a `proc` body.
    Return(ReturnStmt),
    /// A call in statement position: a `proc`, a command function from the
    /// standard library, or one of the body-taking blocks.
    Call(CallStmt),
    /// `receiver.method(args);` — a VMS method used for its effect.
    Method(MethodStmt),
    If(IfStmt),
    Loop(LoopStmt),
    Match(MatchStmt),
    Macro(MacroCall),
    /// `$body;` inside a macro body: the statements a `block` parameter stands
    /// for.
    Param(Ident),
    /// `var x: num = 0;` inside a macro body: a hygienic temporary. Outside a
    /// macro this is an error, because Scratch variables belong to a target.
    Var(VarDecl),
}

impl Stmt {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Stmt::Let(stmt) => stmt.span,
            Stmt::Assign(stmt) => stmt.span,
            Stmt::CompoundAssign(stmt) => stmt.span,
            Stmt::Return(stmt) => stmt.span,
            Stmt::Call(stmt) => stmt.span,
            Stmt::Method(stmt) => stmt.span,
            Stmt::If(stmt) => stmt.span,
            Stmt::Loop(stmt) => stmt.span,
            Stmt::Match(stmt) => stmt.span,
            Stmt::Macro(call) => call.span,
            Stmt::Param(ident) => ident.span,
            Stmt::Var(decl) => decl.span,
        }
    }
}

/// `let x = e;` or `let x: num = e;` — a block-scoped local stored in `_vms`.
///
/// The annotation is optional; without it the cell's type is the value's.
#[derive(Clone, Debug)]
pub struct LetStmt {
    pub name: Ident,
    pub ty: Option<Ty>,
    pub value: Expr,
    pub span: Span,
}

/// `x = e;` or `trail[i] = e;`
#[derive(Clone, Debug)]
pub struct AssignStmt {
    pub target: LValue,
    pub value: Expr,
    pub span: Span,
}

/// `x += e;` — the `op` is always one of `Add`, `Sub`, `Mul`, `Div`, `Rem`.
#[derive(Clone, Debug)]
pub struct CompoundAssignStmt {
    pub target: LValue,
    pub op: BinOp,
    pub value: Expr,
    pub span: Span,
}

/// `return;` or `return e;` — only valid inside a `proc` body.
#[derive(Clone, Debug)]
pub struct ReturnStmt {
    /// `None` for a bare `return;`.
    pub value: Option<Expr>,
    pub span: Span,
}

/// The left side of an assignment: a declared variable followed by the places
/// inside it that the statement writes.
#[derive(Clone, Debug)]
pub struct LValue {
    pub name: Ident,
    /// One accessor per `[…]` or `.field`, outermost first: `p.pos.x` has two.
    pub path: Vec<Accessor>,
    pub span: Span,
}

/// What follows a name in an assignment target or a place expression.
#[derive(Clone, Debug)]
pub enum Accessor {
    /// `name[index]`
    Index(Box<Expr>),
    /// `name.field`
    Field(Ident),
}

impl LValue {
    /// Whether the target is the bare name.
    #[must_use]
    pub fn is_bare(&self) -> bool {
        self.path.is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct IfStmt {
    pub cond: Expr,
    pub then_branch: Block,
    pub else_branch: Option<Block>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct LoopStmt {
    pub kind: LoopKind,
    pub body: Block,
    pub span: Span,
}

/// The three loops Scratch has. `while` and `for` are macros, not variants.
#[derive(Clone, Debug)]
pub enum LoopKind {
    Repeat(Expr),
    RepeatUntil(Expr),
    Forever,
}

/// `match subject { pattern => { … }, … }`
#[derive(Clone, Debug)]
pub struct MatchStmt {
    pub subject: Expr,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct MatchArm {
    /// `None` for the `_` wildcard.
    pub pattern: Option<Pattern>,
    pub body: Block,
    pub span: Span,
}

/// What a `match` arm matches: a literal, or a `const` the checker resolves.
#[derive(Clone, Debug)]
pub enum Pattern {
    Literal(Literal),
    /// A name, which must resolve to a `const`.
    Name(Path),
}

impl Pattern {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Pattern::Literal(literal) => literal.span,
            Pattern::Name(path) => path.span,
        }
    }
}

/// A call in statement position, with an optional body.
///
/// `control::wait(1);` has no body; `control::while(c) { … }` does. The body is
/// only accepted for a catalog block whose `body` is a substack.
#[derive(Clone, Debug)]
pub struct CallStmt {
    pub callee: Path,
    pub args: Vec<Expr>,
    pub body: Option<Block>,
    pub body_span: Span,
    pub span: Span,
}

/// `receiver.name(args);` — a method on a list, a map or another VMS value.
#[derive(Clone, Debug)]
pub struct MethodStmt {
    pub receiver: Expr,
    pub name: Ident,
    pub args: Vec<Expr>,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum Expr {
    /// The exact spelling is preserved: `1.50` does not become `1.5`.
    Number {
        text: String,
        span: Span,
    },
    Str {
        text: String,
        span: Span,
    },
    Bool {
        value: bool,
        span: Span,
    },
    /// A path used as a value: a variable, a parameter, a constant, a menu
    /// variant, or a `const` from another module.
    Name(Path),
    Call(CallExpr),
    /// `list[index]`
    Index {
        list: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    /// `place.field`
    Field {
        base: Box<Expr>,
        name: Ident,
        span: Span,
    },
    /// `receiver.method(args)` — a VMS method, not a Scratch block.
    Method {
        receiver: Box<Expr>,
        name: Ident,
        args: Vec<Expr>,
        span: Span,
    },
    /// `Point { x: 0, y: 0 }` — a struct's fields, in the order written. A
    /// struct is a place, so this is only valid where a place is being made:
    /// the initializer of a `let` or a `var`.
    Struct {
        name: Ident,
        fields: Vec<(Ident, Expr)>,
        span: Span,
    },
    Unary {
        op: UnOp,
        operand: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    /// `f"score: {n}"`
    Interpolated {
        parts: Vec<InterpPart>,
        span: Span,
    },
    Macro(MacroCall),
    /// `$x` inside a macro body.
    Param(Ident),
}

impl Expr {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Expr::Number { span, .. }
            | Expr::Str { span, .. }
            | Expr::Bool { span, .. }
            | Expr::Index { span, .. }
            | Expr::Unary { span, .. }
            | Expr::Binary { span, .. }
            | Expr::Interpolated { span, .. }
            | Expr::Field { span, .. }
            | Expr::Method { span, .. }
            | Expr::Struct { span, .. } => *span,
            Expr::Name(path) => path.span,
            Expr::Call(call) => call.span,
            Expr::Macro(call) => call.span,
            Expr::Param(ident) => ident.span,
        }
    }
}

/// One piece of an `f"…"` string.
#[derive(Clone, Debug)]
pub enum InterpPart {
    Text(String),
    Hole(Expr),
}

/// A call: a standard-library function, an `fn`, or a `proc` used in an
/// expression (which is an error, but is reported by the checker rather than the
/// parser).
#[derive(Clone, Debug)]
pub struct CallExpr {
    pub callee: Path,
    pub args: Vec<Expr>,
    pub span: Span,
}

/// Unary operators, one block each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    /// `!a` → `operator_not`.
    Not,
    /// `-a` → `operator_subtract(0, a)`, or part of a negative literal.
    Neg,
}

/// Binary operators.
///
/// `Le`, `Ge` and `Ne` are here rather than in the prelude because they are
/// spelled as operators in the grammar, but each costs *two* blocks: Scratch has
/// no `≤`, `≥` or `≠`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    /// The catalog opcode this operator lowers to when it is one block.
    ///
    /// `None` for `Ne`, `Le` and `Ge`, which need an `operator_not` around the
    /// block named here.
    #[must_use]
    pub const fn opcode(self) -> Option<&'static str> {
        Some(match self {
            BinOp::Add => "operator_add",
            BinOp::Sub => "operator_subtract",
            BinOp::Mul => "operator_multiply",
            BinOp::Div => "operator_divide",
            BinOp::Rem => "operator_mod",
            BinOp::Eq => "operator_equals",
            BinOp::Ne => return None,
            BinOp::Lt => "operator_lt",
            BinOp::Le => return None,
            BinOp::Gt => "operator_gt",
            BinOp::Ge => return None,
            BinOp::And => "operator_and",
            BinOp::Or => "operator_or",
        })
    }

    /// Whether the operator costs a second block.
    #[must_use]
    pub const fn is_two_blocks(self) -> bool {
        matches!(self, BinOp::Ne | BinOp::Le | BinOp::Ge)
    }

    /// The operator's source spelling.
    #[must_use]
    pub const fn text(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }
}

// ---------------------------------------------------------------------------
// Desugared sugar
// ---------------------------------------------------------------------------

/// A call to a macro, and the representation of every fixed-shape convenience the
/// parser rewrites on the way in.
#[derive(Clone, Debug)]
pub struct MacroCall {
    /// The macro's name, from [`sugar`] when the parser produced it.
    pub name: Ident,
    pub args: Vec<MacroArg>,
    pub span: Span,
}

/// An argument to a macro, matching the parameter kind it fills.
#[derive(Clone, Debug)]
pub enum MacroArg {
    Expr(Expr),
    /// A name the macro is expected to introduce or assign, resolved at the call
    /// site rather than renamed.
    Ident(Ident),
    Block(Block),
    Param(Ident),
}

/// The names the parser desugars surface sugar into.
///
/// These are ordinary macros defined in `std::prelude`, which is the point: the
/// compiler has no special case for `while` or `for`, and `raven expand` prints
/// `for_range(i, a, b, { … })` before it prints what that became.
pub mod sugar {
    /// `while c { … }` → `while_loop(c, { … })`
    pub const WHILE: &str = "while_loop";
    /// `loop { … }` → `loop_forever({ … })`
    pub const LOOP: &str = "loop_forever";
    /// `for i in a..b { … }` → `for_range(i, a, b, { … })`
    pub const FOR: &str = "for_range";
    /// `for i in a..=b { … }` → `for_range_inclusive(i, a, b, { … })`
    pub const FOR_INCLUSIVE: &str = "for_range_inclusive";
    /// `for x in items { … }` → `for_each(x, items, { … })`
    pub const FOR_EACH: &str = "for_each";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_knows_its_prefix_and_its_item() {
        let ident = |name: &str| Ident::new(name, Span::default());
        let path = Path {
            segments: vec![ident("lib"), ident("geometry"), ident("hypot")],
            span: Span::default(),
        };
        assert_eq!(path.last().name, "hypot");
        assert_eq!(
            path.prefix()
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["lib", "geometry"]
        );
        assert_eq!(path.display(), "lib::geometry::hypot");
        assert!(!path.is_single());
    }

    #[test]
    fn only_three_operators_need_a_second_block() {
        let two: Vec<BinOp> = [BinOp::Add, BinOp::Ne, BinOp::Le, BinOp::Ge, BinOp::And]
            .into_iter()
            .filter(|op| op.is_two_blocks())
            .collect();
        assert_eq!(two, [BinOp::Ne, BinOp::Le, BinOp::Ge]);
        assert_eq!(BinOp::Add.opcode(), Some("operator_add"));
        assert_eq!(BinOp::Ne.opcode(), None);
    }

    #[test]
    fn a_literal_knows_its_type() {
        let number = Literal {
            kind: LiteralKind::Number("1.5".into()),
            span: Span::default(),
        };
        assert_eq!(number.ty(), Ty::Num);
        assert!(number.describe().contains("1.5"));
    }
}
