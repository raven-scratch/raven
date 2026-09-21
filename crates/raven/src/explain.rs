//! `raven explain`: the language described for a machine reader.
//!
//! This is not the human guide. It is the same facts written for a language
//! model that has to *emit* raven source and cannot ask questions: exact
//! grammar, exact spellings, exact block costs, the rules that reject code, and
//! the full standard library generated from the binding table rather than
//! retyped. Every section is a plain-text block with a `##` heading, so a
//! caller can ask for one section at a time.
//!
//! It is generated where generation is possible and hand-written where the fact
//! lives only in prose, in which case it is kept next to the constraint: the
//! grammar mirrors `parser.rs`, and the cost table mirrors `lower.rs`.

use crate::identity;
use crate::menu;
use crate::stdlib;
use raven_scratch::catalog::{self, BlockKind, Shape};

/// Every section name, in the order `explain all` prints them.
pub const SECTIONS: &[&str] = &[
    "rules", "grammar", "costs", "types", "memory", "stdlib", "menus", "prelude", "cli",
];

/// Print one section, or all of them.
pub fn print(section: &str) -> Result<(), String> {
    if section == "all" {
        for name in SECTIONS {
            print!("{}", text(name));
        }
        return Ok(());
    }
    if SECTIONS.contains(&section) {
        print!("{}", text(section));
        return Ok(());
    }
    Err(format!(
        "unknown section `{section}`; the sections are: all, {}",
        SECTIONS.join(", ")
    ))
}

/// One section, as text. Panics only on a name that is not in [`SECTIONS`].
#[must_use]
pub fn text(section: &str) -> String {
    match section {
        "rules" => rules(),
        "grammar" => grammar(),
        "costs" => costs(),
        "types" => types(),
        "memory" => memory(),
        "stdlib" => stdlib_section(),
        "menus" => menus(),
        "prelude" => prelude(),
        "cli" => cli(),
        other => unreachable!("unknown section `{other}`"),
    }
}

fn rules() -> String {
    format!(
        "\n## rules\n\
         # {crate} {version} — a machine-readable reference for a model that has to emit\n\
         # .rav source. Layers: raven (.rav) lowers to raven-asm (.rasm), which compiles to\n\
         # project.json and then to a .sb3. `raven expand` prints the middle layer.\n\
         # docs: {docs}\n\
         #\n\
         # A file is either one target (`stage {{ … }}` or `sprite \"Name\" {{ … }}`) or a module\n\
         # (no target, items other files import). A module cannot give a target a costume, a\n\
         # sound or a sprite-local variable.\n\
         # There is no top-level statement: every statement is inside an `on <hat> {{ … }}`\n\
         # script or a `proc` body.\n\
         # `var`, `const`, `costume`, `sound` and `broadcast` are declared beside the other\n\
         # declarations, never inside a script. A local is `let`.\n\
         # Sprite names are unique and `Stage` is reserved: `sprite \"Stage\"` is an error.\n\
         # `return` is only allowed in a `proc` that declares `-> <type>`.\n\
         # There is no `break`, no `continue`, no conditional expression, no `as` cast, no\n\
         # attributes and no string `+`: use `if`, `num(x)`, `str(x)` and `f\"…\"`.\n\
         # `&&` and `||` are eager; comparisons do not chain (`a < b < c` is an error).\n\
         # A statement-only method has no value: `l.push`, `l.pop`, `l.insert`, `l.remove`,\n\
         # `l.clear`, `m.set`, `m.remove`, `m.clear`. `let v = l.pop();` is an error; the value\n\
         # form is `let v = l.last(); l.pop();`.\n\
         # A macro may not nest inside itself, so a `for` may not contain a `for`; write the\n\
         # outer loop as `repeat n {{ … }}`. Expansion is capped at 32 levels deep.\n\
         # `pub` is meaningful on `var`, `list`, `const`, `fn`, `macro`, `proc` and `struct`;\n\
         # a `pub struct` parses but cannot be imported.\n\
         # A program cannot name a Scratch variable. `watch a, b;` is the one exception: it\n\
         # declares a visible Scratch variable per name, to be looked at, and nothing else.\n\
         # The five blocks that name a Scratch variable are refused with a note saying what\n\
         # to write instead.\n\
         # A struct occupies at most 64 cells and is a place, not a value: build it in a\n\
         # `let`/`var` initializer, write its fields, never copy it.\n\
         # Initializers are literals, lists of literals or struct literals; nothing is\n\
         # computed at compile time.\n\
         # `use` comes before every other item, and importing a module brings all of its\n\
         # public items into scope.\n\
         # Every convenience is a rewrite `raven expand` prints. If it cannot be printed, it\n\
         # is not in the language.\n\
         # Three things can be called. `fn` is a compile-time substitution: no statements,\n\
         # inlined at every call site, free at run time. `macro` is the same with expression,\n\
         # name and block parameters. `proc` is a real Scratch custom block whose body exists\n\
         # once and is shared by every caller, may hold any statement, and costs a call plus a\n\
         # cell read when it declares `-> ty`.\n",
        crate = identity::CRATE,
        version = env!("CARGO_PKG_VERSION"),
        docs = identity::DOCS,
    )
}

fn grammar() -> String {
    "
## grammar
# EBNF. `#` starts a comment; whitespace is insignificant.
# Items are not terminated; statements are.

file        = { use } { item }
use         = \"use\" path [ \"::\" \"{\" ident { \",\" ident } \"}\" ] \";\"
item        = target | var_decl | const_decl | struct_decl | broadcast | costume
            | sound | proc_def | fn_def | macro_def | script | watch

target      = \"stage\" block | \"sprite\" STRING block
var_decl    = [ \"pub\" ] \"var\" IDENT \":\" type \"=\" initializer \";\"
const_decl  = [ \"pub\" ] \"const\" IDENT \":\" type \"=\" literal \";\"
struct_decl = [ \"pub\" ] \"struct\" IDENT \"{\" { IDENT \":\" type [ \",\" ] } \"}\"
broadcast   = \"broadcast\" STRING \";\"
costume     = \"costume\" STRING \"=\" STRING [ \"center\" STRING STRING ] \";\"
sound       = \"sound\" STRING \"=\" STRING \";\"
watch       = \"watch\" IDENT { \",\" IDENT } \";\"
initializer = literal | \"[\" [ literal { \",\" literal } ] \"]\" | struct_literal

type        = \"num\" | \"str\" | \"bool\" | \"list\" \"<\" scalar \">\"
            | \"map\" \"<\" scalar \",\" scalar \">\" | IDENT
scalar      = \"num\" | \"str\" | \"bool\"

proc_def    = \"proc\" IDENT \"(\" [ params ] \")\" [ \"->\" type ] [ \"warp\" ] block
params      = param { \",\" param }
param       = IDENT \":\" scalar
fn_def      = \"fn\" IDENT \"(\" [ fn_params ] \")\" \"->\" type \"{\" expr \"}\"
fn_params   = IDENT \":\" type { \",\" IDENT \":\" type }
macro_def   = \"macro\" IDENT \"(\" [ macro_params ] \")\" \"->\" macro_result ( \"{\" expr \"}\" | block )
macro_params= \"$\" IDENT \":\" macro_kind
macro_kind  = \"expr\" [ \"<\" type \">\" ] | \"ident\" | \"block\"
macro_result= type | \"stmts\"
script      = \"on\" hat block
hat         = IDENT [ \"(\" [ expr { \",\" expr } ] \")\" ]

stmt        = let | assign | op_assign | return | if_stmt | repeat_stmt
            | repeat_until | forever | while_stmt | for_stmt | match_stmt | call \";\"
let         = \"let\" IDENT [ \":\" type ] \"=\" expr \";\"
assign      = lvalue \"=\" expr \";\"
lvalue      = IDENT | index | field
index       = IDENT \"[\" expr \"]\"
field       = IDENT { \".\" IDENT }
op_assign   = lvalue ( \"+=\" | \"-=\" | \"*=\" | \"/=\" | \"%=\" ) expr \";\"
return      = \"return\" [ expr ] \";\"
if_stmt     = \"if\" expr block [ \"else\" ( if_stmt | block ) ]
repeat_stmt = \"repeat\" expr block
repeat_until= \"repeat_until\" expr block
forever     = \"forever\" block
while_stmt  = \"while\" expr block
for_stmt    = \"for\" IDENT \"in\" expr \"..\" expr block
match_stmt  = \"match\" expr \"{\" { pattern \"=>\" block } \"}\"
pattern     = literal | IDENT | \"_\"
call        = path \"(\" [ expr { \",\" expr } ] \")\"

expr        = or
or          = and { \"||\" and }
and         = cmp { \"&&\" cmp }
cmp         = sum [ ( \"==\" | \"!=\" | \"<\" | \"<=\" | \">\" | \">=\" ) sum ]
sum         = product { ( \"+\" | \"-\" ) product }
product     = unary { ( \"*\" | \"/\" | \"%\" ) unary }
unary       = [ \"!\" | \"-\" ] postfix
postfix     = primary { \"[\" expr \"]\" | \".\" IDENT [ \"(\" [ expr { \",\" expr } ] \")\" ] }
primary     = literal | interp | path [ \"(\" [ expr { \",\" expr } ] \")\" ]
            | \"num\" \"(\" expr \")\" | \"str\" \"(\" expr \")\"
            | struct_literal | \"$\" IDENT | \"(\" expr \")\"

literal     = NUMBER | STRING | \"true\" | \"false\"
interp      = 'f\"' { text | \"{\" expr \"}\" } '\"'
path        = IDENT { \"::\" ( IDENT | keyword ) }
block       = \"{\" { stmt } \"}\"

# keywords: bool broadcast const costume else false fn for forever if in let list
#   macro map match num on proc pub repeat repeat_until return sound sprite stage
#   str struct true use var warp watch while
# punctuation: ( ) { } [ ] , ; : :: = -> => . .. ! && || == != <= >= < > + - * / %
"
    .to_string()
}

fn costs() -> String {
    "
## costs
# Blocks emitted per construct, beyond the code written inside it. These are the
# same numbers `raven expand` shows; they are the contract, not an estimate.
# `_stackN` is a per-script stack list; `_vms` is the target arena; `_gvm` is the
# stage arena. A `let` outside a `proc` is a `_stackN` push and a pop when its
# block ends; inside a `proc` it is a `_vms` cell.

stmt                 blocks   note
x = e                1        _stackN for a let, _vms for a var, _gvm for a pub var
p.x = e              1        one cell at the field's constant offset
l[i] = e             6        a grow to index i (5) then the replace (1)
x += e               3        read, operator, write
l.push(e)            1        data_addtolist
l.pop()              2        data_deleteoflist at (length of l)
l.insert(i, e)       1
l.remove(i)          1
l.clear()            1
m.set(k, v)          6 + cell
m.remove(k)          5 + cell
return e             2        arena cell write + control_stop(\"this script\")
if / if-else         1
repeat / repeat_until / forever  1
while c              2        operator_not + control_repeat_until
for i in a..b        9 + cell counter on _stackN
match e              1 per arm, plus 1 and a cell when the subject is sampled
proc call            1
reporter call        1 per block in the expression

expr                 blocks   note
a + b - * / %        1 each
-a                   1
a == b               1
a != b, a <= b, a >= b  2   Scratch has no not-equal, no <= and no >=
a && b, a || b       1        both sides evaluated, always
!a                   1
l[i], l.at(i), l.first()     1
l.last()             2        item (length of l) of l
l.text()             1        data_listcontents
l.is_empty(), m.is_empty()   2
m.has(k)             2
m.len()              2
m.get(k)             5 + 2 cells  (a guarded read; see docs/raven/lowering)
f\"…\"                one operator_join per piece after the first
num(x), str(x)       0        a retype, no block
fn call              the body's blocks, inlined at the call site
proc call with ->    the call, then a cell copy and a read (2)
"
    .to_string()
}

fn types() -> String {
    "
## types
# num, str, bool, list<num|str|bool>, map<num|str|bool, num|str|bool>, and
# declared structs. There is no inference from use: a `var` states its type, a
# `let` may state it or take it from the initializer.
# Two conversions exist and both are free: num(x) and str(x), between num and str.
# There is no bool(x); write the comparison you meant.
# Scratch shapes, and what raven accepts for each:
#   Number, Positive, Whole, Integer, Angle  <- num
#   Text                                     <- str and num
#   Bool                                     <- bool only
#   Color                                    <- \"#rrggbb\"
#   Broadcast                                <- a declared broadcast name
#   Menu(id)                                 <- the RavenType the menu table names
#   Variable, List                           <- not expression slots; use the declaration
# A stored boolean is read back as <cell = \"true\">; a literal boolean is the
# constant comparison <1 = 1>. A boolean may live in a cell, a list or a map.
# A struct is a place: `let p: Point = Point { x: 0, y: 0 };` makes a frame of
# cells, `p.x` reads one cell, and a struct cannot be copied or passed.
"
    .to_string()
}

fn memory() -> String {
    "
## memory
# A raven program declares no Scratch variable a program can name. Every value it
# stores is a cell of a Scratch list, addressed by a constant index chosen at
# compile time.
#   _vms    one per target: its `var`s and every `proc` frame. Grown on demand by
#           a generated `__vms_reserve` warp procedure.
#   _gvm    declared on the stage: every `pub var`, and every stage `var`.
#   _stackN one per script that keeps block-scoped state. A `let` outside a `proc`
#           pushes onto it and the matching `data_deleteoflist` pops when the
#           block ends. Numbering starts at _stack1.
#   _console the log, declared only when something logs.
# None of them exists unless used. A `watch` is the one place a real Scratch
# variable is declared, and it exists to be displayed.
# Reading a cell is data_itemoflist; writing one is data_replaceitemoflist. A
# `let` in a script is data_addtolist + data_deleteoflist, not a replace.
# Lists and maps are Scratch lists: `list<T>` is one list, `map<K,V>` is one list
# of alternating keys and values.
"
    .to_string()
}

fn prelude() -> String {
    format!(
        "\n## prelude\n# Imported into every file. Every item can be read, shadowed or replaced.\n\
         # Source: crates/raven/src/prelude.rav\n\n{}",
        include_str!("prelude.rav")
    )
}

fn cli() -> String {
    format!(
        "\n## cli\n\
         # raven new <name>       scaffold a project (--here, --force, --with-module)\n\
         # raven init [path]      scaffold into an existing directory (--force)\n\
         # raven check            lex, parse, resolve, check, expand; writes nothing\n\
         # raven expand           print the raven-asm the project lowers to\n\
         # raven build            write dist/<name>.sb3 (--debug, -m/--manifest)\n\
         # raven fmt              canonical indentation (--check, paths)\n\
         # raven clean            remove the output directory\n\
         # raven explain [section] this reference; sections: all, {}\n\
         # raven-asm build|check|catalog|new|init|clean  the layer below; its own CLI\n\
         # `raven build --debug` also writes dist/asm/ (a project raven-asm can build)\n\
         # and dist/project.json. Exit codes: 0 success, 1 diagnostic, 2 usage.\n\
         # docs: {}\n",
        SECTIONS.join(", "),
        identity::DOCS
    )
}

/// One line per catalog block: opcode, raven spelling, arguments, result.
fn stdlib_section() -> String {
    let mut out = String::from(
        "\n## stdlib\n\
         # Every catalog block, in catalog order, with the raven spelling the compiler\n\
         # enforces. Format: <opcode> :: <spelling> | args: <name:shape,…> | result: <ty|none> \
         | body: <none|next|substack|else> | <kind>\n\
         # `REFUSED` means the block is reachable from the catalog and deliberately not\n\
         # callable; the reason follows.\n\
         # A block whose raven name is unambiguous may also be called without its module.\n\
         # Hats are written as `on <name> { … }`; `Syntax` entries are covered by grammar.\n",
    );
    for block in catalog::BLOCKS {
        let Some(row) = stdlib::BINDINGS.iter().find(|r| r.opcode == block.opcode) else {
            continue;
        };
        let args: Vec<String> = block
            .args
            .iter()
            .map(|a| format!("{}:{}", a.name, shape_name(a.shape)))
            .collect();
        let result = row
            .binding
            .result()
            .map_or_else(|| "none".to_string(), |v| v.name().to_string());
        let body = match block.body {
            catalog::Body::None => "none",
            catalog::Body::Next => "next",
            catalog::Body::Substack => "substack",
            catalog::Body::SubstackElse => "substack+else",
        };
        let refused = row
            .binding
            .forbidden()
            .map_or(String::new(), |why| format!(" | REFUSED: {why}"));
        out.push_str(&format!(
            "{} :: {} | args: {} | result: {result} | body: {body} | {}{refused}\n",
            block.opcode,
            row.binding.spelling(),
            args.join(", "),
            kind_name(block.kind),
        ));
    }
    out
}

fn menus() -> String {
    let mut out = String::from(
        "\n## menus\n\
         # A dropdown is an enum type. `<MenuType>::<Variant>` is the only spelling for a\n\
         # closed menu; a typo is a compile error. `acceptReporters` menus also take an\n\
         # expression of the same type.\n",
    );
    for id in menu::menu_ids() {
        let name = menu::type_name(id);
        let description = match menu::domain(id) {
            menu::Domain::Fixed(values) => {
                let variants: Vec<String> = values.iter().map(|v| menu::variant(v)).collect();
                format!("variants: {}", variants.join(", "))
            }
            menu::Domain::Sprites(extras) => {
                let extras: Vec<String> = extras
                    .iter()
                    .map(|v| menu::variant(v).to_string())
                    .collect();
                format!(
                    "a sprite name in the project, or one of: {}",
                    extras.join(", ")
                )
            }
            menu::Domain::Costumes => "the target's costume names".to_string(),
            menu::Domain::Backdrops => "the stage's backdrop names".to_string(),
            menu::Domain::Sounds => "the target's sound names".to_string(),
            menu::Domain::Open => "open: any literal; this menu is not enumerable".to_string(),
        };
        out.push_str(&format!("{id} -> {name} :: {description}\n"));
    }
    out
}

fn shape_name(shape: Shape) -> String {
    match shape {
        Shape::Number | Shape::Positive | Shape::Whole | Shape::Integer | Shape::Angle => {
            "num".to_string()
        }
        Shape::Text => "str|num".to_string(),
        Shape::Bool => "bool".to_string(),
        Shape::Color => "#rrggbb".to_string(),
        Shape::Variable => "variable-name".to_string(),
        Shape::List => "list-name".to_string(),
        Shape::Broadcast => "broadcast-name".to_string(),
        Shape::Menu(id) => menu::type_name(id),
        Shape::ParamName => "parameter-name".to_string(),
    }
}

fn kind_name(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::Hat => "hat",
        BlockKind::Stack => "stack",
        BlockKind::Cap => "cap",
        BlockKind::Reporter => "reporter",
        BlockKind::Boolean => "boolean",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_section_prints_and_carries_its_heading() {
        for section in SECTIONS {
            let text = text(section);
            assert!(
                text.starts_with(&format!("\n## {section}\n")),
                "`{section}` must start with its heading"
            );
            assert!(text.len() > 200, "`{section}` is suspiciously short");
        }
    }

    #[test]
    fn the_stdlib_section_covers_every_binding() {
        let text = text("stdlib");
        for row in stdlib::BINDINGS {
            assert!(
                text.contains(&format!("{} :: ", row.opcode)),
                "`{}` is missing from `raven explain stdlib`",
                row.opcode
            );
        }
    }

    #[test]
    fn the_menus_section_covers_every_menu() {
        let text = text("menus");
        for id in menu::menu_ids() {
            assert!(
                text.contains(&format!("{id} -> ")),
                "menu `{id}` is missing from `raven explain menus`"
            );
        }
    }

    #[test]
    fn an_unknown_section_is_an_error_that_lists_the_real_ones() {
        let error = print("nonesuch").expect_err("must fail");
        assert!(error.contains("unknown section `nonesuch`"), "{error}");
        assert!(error.contains("grammar"), "{error}");
    }
}
