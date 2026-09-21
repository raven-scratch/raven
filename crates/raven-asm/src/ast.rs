//! Abstract syntax tree for raven-asm.
//!
//! The AST is intentionally shallow: a raven-asm statement *is* a Scratch block
//! invocation, so there is no desugaring step between the tree and the
//! generated `project.json`.

use raven_scratch::diag::Pos;

/// A literal value as written in the source. Kept as text wherever Scratch
/// keeps text, so `var score = 1.50;` round-trips exactly.
#[derive(Clone, Debug)]
pub enum Literal {
    Number(String),
    Str(String),
    Bool(bool),
}

impl Literal {
    /// The JSON value Scratch stores for this literal.
    ///
    /// Scratch keeps a variable's type: a number literal becomes a JSON number
    /// (so `1.50` normalizes to `1.5`), a boolean becomes a JSON boolean, and a
    /// string stays a string.
    pub fn json(&self) -> serde_json::Value {
        match self {
            Literal::Number(raw) => raw
                .parse::<serde_json::Number>()
                .map(serde_json::Value::Number)
                .unwrap_or_else(|_| serde_json::Value::String(raw.clone())),
            Literal::Str(s) => serde_json::Value::String(s.clone()),
            Literal::Bool(b) => serde_json::Value::Bool(*b),
        }
    }
}

/// A parsed source file.
#[derive(Clone, Debug)]
pub struct File {
    pub uses: Vec<UseDecl>,
    /// Present when this file declares a stage or a sprite.
    pub target: Option<TargetDecl>,
    /// Module-level items; only allowed when `target` is `None`.
    pub items: Vec<Item>,
}

#[derive(Clone, Debug)]
pub struct UseDecl {
    pub path: String,
    pub pos: Pos,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetKind {
    Stage,
    Sprite,
}

#[derive(Clone, Debug)]
pub struct TargetDecl {
    pub kind: TargetKind,
    pub name: String,
    pub pos: Pos,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug)]
pub enum Item {
    Var(VarDecl),
    List(ListDecl),
    Broadcast(BroadcastDecl),
    Costume(CostumeDecl),
    Sound(SoundDecl),
    Proc(ProcDecl),
    Stmt(Stmt),
}

impl Item {
    pub fn pos(&self) -> Pos {
        match self {
            Item::Var(v) => v.pos,
            Item::List(l) => l.pos,
            Item::Broadcast(b) => b.pos,
            Item::Costume(c) => c.pos,
            Item::Sound(s) => s.pos,
            Item::Proc(p) => p.pos,
            Item::Stmt(s) => s.pos,
        }
    }

    pub fn describe(&self) -> &'static str {
        match self {
            Item::Var(_) => "variable",
            Item::List(_) => "list",
            Item::Broadcast(_) => "broadcast message",
            Item::Costume(_) => "costume",
            Item::Sound(_) => "sound",
            Item::Proc(_) => "procedure",
            Item::Stmt(_) => "block",
        }
    }
}

/// How a monitor is drawn, which is the `mode` of its record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MonitorMode {
    /// The ordinary readout.
    #[default]
    Default,
    /// The large readout: one number, no label.
    Large,
    /// A draggable slider.
    Slider,
}

/// Everything a declaration can say about its monitor.
///
/// The editor reads these straight out of `project.json`: `x`/`y` are stage
/// pixels and a `null` position asks the editor to place the monitor itself,
/// exactly as it does for a variable created in the editor.
#[derive(Clone, Copy, Debug, Default)]
pub struct MonitorSpec {
    /// `at X Y`, when the declaration places its own monitor.
    pub at: Option<(f64, f64)>,
    /// `default`, `large` or `slider`.
    pub mode: MonitorMode,
    /// `slider MIN MAX`.
    pub slider: Option<(f64, f64)>,
    /// `continuous`, which makes a slider's step 0.01 instead of 1.
    pub continuous: bool,
}

#[derive(Clone, Debug)]
pub struct VarDecl {
    /// `global var x = 0;` — the variable belongs to the stage, so every sprite
    /// can see it, wherever it was declared.
    pub global: bool,
    /// `visible var x = 0;` — the editor's monitor for it starts shown, which
    /// is how a value that is not a Scratch variable gets on the stage.
    pub visible: bool,
    /// Where the monitor goes and how it is drawn.
    pub monitor: MonitorSpec,
    pub name: String,
    pub init: Literal,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct ListDecl {
    /// `global list x = [];` — see [`VarDecl::global`].
    pub global: bool,
    /// `visible list x = [];` — see [`VarDecl::visible`].
    pub visible: bool,
    /// A list monitor is always a list; `at` still says where it sits.
    pub monitor: MonitorSpec,
    pub name: String,
    pub init: Vec<Literal>,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct BroadcastDecl {
    pub name: String,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct CostumeDecl {
    pub name: String,
    pub path: String,
    /// Explicit rotation centre in costume pixels, when given with `center x y`.
    pub center: Option<(f64, f64)>,
    pub pos: Pos,
    pub path_pos: Pos,
}

#[derive(Clone, Debug)]
pub struct SoundDecl {
    pub name: String,
    pub path: String,
    pub pos: Pos,
    pub path_pos: Pos,
}

/// The kind of Scratch custom-block input a parameter maps to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    /// `%s` — a string/number input, read with `argument_reporter_string_number`.
    String,
    /// `%n` — a number input, read with `argument_reporter_string_number`.
    Number,
    /// `%b` — a boolean input, read with `argument_reporter_boolean`.
    Boolean,
}

impl ParamKind {
    /// How the type is written in source and reported in diagnostics.
    pub fn spelling(self) -> &'static str {
        match self {
            ParamKind::String => "str",
            ParamKind::Number => "num",
            ParamKind::Boolean => "bool",
        }
    }

    pub fn proccode_letter(self) -> char {
        match self {
            ParamKind::String => 's',
            ParamKind::Number => 'n',
            ParamKind::Boolean => 'b',
        }
    }
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub kind: ParamKind,
}

#[derive(Clone, Debug)]
pub struct ProcDecl {
    pub name: String,
    pub params: Vec<Param>,
    /// `proc name(...) warp { }` — the custom block runs without screen refresh.
    pub warp: bool,
    pub body: Vec<Stmt>,
    pub pos: Pos,
}

/// A single Scratch block invocation.
#[derive(Clone, Debug)]
pub struct Stmt {
    /// Scratch opcode, or a user procedure name for custom-block calls.
    pub opcode: String,
    pub args: Vec<Expr>,
    /// `SUBSTACK` body, when the block is written with braces.
    pub body: Option<Vec<Stmt>>,
    /// `SUBSTACK2` body, for `else { }`.
    pub else_body: Option<Vec<Stmt>>,
    pub pos: Pos,
    /// Length of the opcode token, for error underlining.
    pub len: u32,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Number(String, Pos),
    Str(String, Pos),
    Bool(bool, Pos),
    /// A reporter block invocation.
    Call(CallExpr),
}

impl Expr {
    pub fn pos(&self) -> Pos {
        match self {
            Expr::Number(_, p) | Expr::Str(_, p) | Expr::Bool(_, p) => *p,
            Expr::Call(c) => c.pos,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Expr::Number(raw, _) => format!("number `{raw}`"),
            Expr::Str(s, _) => format!("string \"{s}\""),
            Expr::Bool(b, _) => format!("`{b}`"),
            Expr::Call(c) => format!("`{}`", c.opcode),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CallExpr {
    pub opcode: String,
    pub args: Vec<Expr>,
    pub pos: Pos,
    pub len: u32,
}
