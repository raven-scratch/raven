//! The language, page by page.
//!
//! These pages are hand-written, because the fact lives in prose, and they are
//! kept next to the constraint they describe: `GRAMMAR` mirrors `parser.rs`,
//! `COSTS` mirrors `lower.rs`, and `TYPES` mirrors `types.rs`. `library.rs`
//! holds the two pages that are generated instead.

/// `rules` — everything that rejects code, first, because that is what a
/// generator trips on.
pub const RULES: &str = "\n## rules\n\
# The restrictions, in the order a generator meets them. Every one of these is an\n\
# error, not a warning.\n\
#\n\
# # Shape of a file\n\
#\n\
# A .rav file is either ONE target (`stage { … }` or `sprite \"Name\" { … }`) or a\n\
# module (no target, items other files import). A module cannot give a target a\n\
# costume a target will not wear, a sound, or a sprite-local variable.\n\
# There is no top-level statement. Every statement is inside an `on <hat> { … }`\n\
# script or a `proc` body, because a loose stack in Scratch never runs.\n\
# Sprite names are unique and `Stage` is reserved: `sprite \"Stage\"` is an error.\n\
# Every target needs at least one `costume` declaration.\n\
# Every `use` comes before the first item.\n\
#\n\
# # Declarations\n\
#\n\
# `var`, `const`, `struct`, `costume`, `sound` and `broadcast` are declared beside\n\
# the other declarations, never inside a script and never inside a `proc`. A local\n\
# is `let`. A `var` statement is legal only inside a macro body, where it declares\n\
# a cell for that expansion.\n\
# A module file may declare only `pub` items (a `costume` is the one exception: a\n\
# module's costumes are worn by every target that uses it). A non-`pub` `var` in a\n\
# module is an error, because a module has no target for the name to belong to.\n\
# `pub` is meaningful on `var`, `list`, `const`, `fn`, `macro`, `proc` and\n\
# `struct`, and rejected on anything else — a `watch`, a `broadcast`, a `costume`,\n\
# a `sound`, an `on` script.\n\
# A `pub struct` parses, but it cannot travel: a struct is resolved inside the\n\
# target that declares it, so it is not usable through `use`.\n\
# There is no shadowing between `var`s: a file's own declaration may not share a\n\
# name with an imported one.\n\
# A struct occupies at most 64 cells, fields and nested fields together.\n\
# Initializers are literals, lists of literals or struct literals; nothing is\n\
# computed at compile time. There is no compile-time evaluation anywhere.\n\
#\n\
# # Decorators\n\
#\n\
# A decorator is written on the line above a `var`, one per line, as `@name` or\n\
# `@name(args)`. Exactly two exist: `@scratch_global` stores the declaration as a\n\
# real Scratch variable or list on the stage, and `@scratch_sprite` as one of the\n\
# declaring sprite's own.\n\
# Either takes no arguments, goes on nothing but a `var`, and cannot be repeated or\n\
# combined with the other. `@scratch_sprite` is refused on the stage (whose\n\
# variables are project-wide) and in a module, and a `pub @scratch_sprite` is\n\
# refused because a sprite-local value cannot be imported. A `struct` cannot take\n\
# either: a struct is a run of cells and a Scratch variable is one cell.\n\
#\n\
# # Naming state\n\
#\n\
# An undecorated `var` or `list` is a cell or run of `_vms`, the one arena the\n\
# stage declares. `pub` decides who may name it, never where it lives.\n\
# A program cannot name a Scratch variable. `watch a, b;` declares a visible\n\
# Scratch variable per name, to be looked at, and nothing else; its monitor starts\n\
# shown. Anything watched must be declared in the target that watches it, or be a\n\
# `pub var`.\n\
# The five blocks that name a Scratch variable (the `data_*` family that takes a\n\
# variable rather than a list) are refused with a note saying what to write\n\
# instead.\n\
#\n\
# # Types and ownership\n\
#\n\
# Every `var` states its type; a `let` may state it or take it from the\n\
# initializer. There is no inference from later use.\n\
# The only conversions are `num(x)` and `str(x)`, and both are free. There is no\n\
# `bool(x)`; write the comparison you meant.\n\
# `list<T>` and `map<K, V>` are owned: they cannot be assigned, compared, returned\n\
# or passed, `let a = b;` is refused, and a list cannot be a `proc` parameter or a\n\
# result. A `struct` is a place, not a value: it cannot be assigned, compared,\n\
# returned or passed either, and it cannot live in a list or a map. A struct field\n\
# may be another struct; it may not be a list or a map.\n\
# A struct literal is only written where a place is being made — the initializer of\n\
# a `let` or a `var`.\n\
# A `let` inside a *recursive* procedure is refused, because a cell is shared by\n\
# every call of the procedure. Pass the value as a parameter instead.\n\
#\n\
# # Statements\n\
#\n\
# There is no `break`, no `continue`, no conditional expression, no `as` cast, no\n\
# general attributes and no string `+`: use `if`, `num(x)`, `str(x)` and `f\"…\"`.\n\
# The one thing shaped like an attribute is a decorator on a `var`.\n\
# `return` is only allowed in a `proc` that declares `-> <type>`.\n\
# `&&` and `||` are eager: both sides are always evaluated. Comparisons do not\n\
# chain (`a < b < c` is an error).\n\
# A statement-only method has no value: `l.push`, `l.pop`, `l.insert`, `l.remove`,\n\
# `l.clear`, `m.set`, `m.remove`, `m.clear`. `let v = l.pop();` is an error; the\n\
# value form is `let v = l.last(); l.pop();`. Which methods exist depends on the\n\
# receiver's type.\n\
# `for x in items` takes the list's NAME, not an expression: the macro reads the\n\
# list's length each turn, so `items.at(2)` is refused.\n\
#\n\
# # Macros and procedures\n\
#\n\
# A macro may not nest inside itself, so a macro whose own definition calls it is a\n\
# cycle and is refused. A `for` inside a `for` is an ordinary program: the call is\n\
# a step of that expansion, a name arriving through a substituted `$body` belongs\n\
# to the caller. Expansion stops at 32 levels deep, which is also an error.\n\
# A macro parameter used in more than one statement of the expansion must be given\n\
# an expression that may be evaluated more than once: `twice(sensing::timer())` is\n\
# an error, `twice(2)` is not. Uses inside one statement are always safe.\n\
# An `fn` may not contain statements. It is a substitution, so it cannot loop,\n\
# branch or recurse.\n\
# Module imports must be acyclic.\n\
#\n\
# # What is deliberately absent\n\
#\n\
#   if c { 1 } else { 2 }     no conditional expression; use an if statement\n\
#   break; continue;           Scratch cannot leave a loop without ending the script\n\
#   var y: num = 0;            inside a proc: use `let`, or declare it on the target\n\
#   a as num                   no `as`; write num(a)\n\
#   #[warp]                    no general attributes; a decorator goes on a `var`\n\
#   \"score: \" + score          no string +; write f\"score: {score}\"\n\
#   bool(x)                    no meaning; write the comparison you meant\n\
#   l.pop() as a value         statement-only methods have no value\n\
#\n\
# Three things can be called. `fn` is a compile-time substitution: no statements,\n\
# inlined at every call site, free at run time. `macro` is the same with\n\
# expression, name and block parameters. `proc` is a real Scratch custom block\n\
# whose body exists once and is shared by every caller, may hold any statement, and\n\
# costs a call plus a cell read when it declares `-> ty`.\n";

/// `grammar` — the whole EBNF.
pub const GRAMMAR: &str = "\n## grammar\n\
# EBNF. `#` is not a comment here; `//` starts a line comment and `/* … */` a\n\
# block comment in the language itself. Whitespace is insignificant.\n\
# Items are not terminated; statements are.\n\
\n\
file        = { use } { item }\n\
use         = \"use\" path [ \"::\" \"{\" ident { \",\" ident } \"}\" ] \";\"\n\
item        = { decorator } ( target | var_decl | const_decl | struct_decl | broadcast\n\
            | costume | sound | proc_def | fn_def | macro_def | script | watch )\n\
decorator   = \"@\" IDENT [ \"(\" [ expr { \",\" expr } ] \")\" ]\n\
\n\
target      = \"stage\" block | \"sprite\" STRING block\n\
var_decl    = [ \"pub\" ] \"var\" ( IDENT | \"$\" IDENT ) \":\" type \"=\" initializer \";\"\n\
const_decl  = [ \"pub\" ] \"const\" IDENT \":\" type \"=\" literal \";\"\n\
struct_decl = [ \"pub\" ] \"struct\" IDENT \"{\" { IDENT \":\" type [ \",\" ] } \"}\"\n\
broadcast   = \"broadcast\" STRING \";\"\n\
costume     = \"costume\" STRING \"=\" STRING [ \"center\" STRING STRING ] \";\"\n\
sound       = \"sound\" STRING \"=\" STRING \";\"\n\
watch       = \"watch\" IDENT { \",\" IDENT } \";\"\n\
initializer = literal | \"[\" [ literal { \",\" literal } ] \"]\" | struct_literal\n\
\n\
type        = \"num\" | \"str\" | \"bool\" | \"list\" \"<\" scalar \">\"\n\
            | \"map\" \"<\" scalar \",\" scalar \">\" | IDENT\n\
scalar      = \"num\" | \"str\" | \"bool\"\n\
\n\
proc_def    = \"proc\" IDENT \"(\" [ params ] \")\" [ \"->\" type ] [ \"warp\" ] block\n\
params      = param { \",\" param }\n\
param       = IDENT \":\" scalar\n\
fn_def      = \"fn\" IDENT \"(\" [ fn_params ] \")\" \"->\" type \"{\" expr \"}\"\n\
fn_params   = IDENT \":\" type { \",\" IDENT \":\" type }\n\
macro_def   = \"macro\" IDENT \"(\" [ macro_params ] \")\" \"->\" macro_result ( \"{\" expr \"}\" | block )\n\
macro_params= \"$\" IDENT \":\" macro_kind\n\
macro_kind  = \"expr\" [ \"<\" type \">\" ] | \"ident\" | \"block\"\n\
macro_result= type | \"stmts\"\n\
script      = \"on\" hat block\n\
hat         = IDENT [ \"(\" [ expr { \",\" expr } ] \")\" ]\n\
\n\
stmt        = let | assign | op_assign | return | if_stmt | repeat_stmt\n\
            | repeat_until | forever | loop_stmt | while_stmt | for_stmt\n\
            | match_stmt | call \";\"\n\
let         = \"let\" IDENT [ \":\" type ] \"=\" expr \";\"\n\
assign      = lvalue \"=\" expr \";\"\n\
lvalue      = IDENT | \"$\" IDENT | index | field\n\
index       = IDENT \"[\" expr \"]\"\n\
field       = IDENT { \".\" IDENT }\n\
op_assign   = lvalue ( \"+=\" | \"-=\" | \"*=\" | \"/=\" | \"%=\" ) expr \";\"\n\
return      = \"return\" [ expr ] \";\"\n\
if_stmt     = \"if\" expr block [ \"else\" ( if_stmt | block ) ]\n\
repeat_stmt = \"repeat\" expr block\n\
repeat_until= \"repeat_until\" expr block\n\
forever     = \"forever\" block\n\
loop_stmt   = \"loop\" block\n\
while_stmt  = \"while\" expr block\n\
for_stmt    = \"for\" IDENT \"in\" ( expr \"..\" expr | expr \"..=\" expr | expr ) block\n\
match_stmt  = \"match\" expr \"{\" { pattern \"=>\" block } \"}\"\n\
pattern     = literal | IDENT | \"_\"\n\
call        = path \"(\" [ expr { \",\" expr } ] \")\"\n\
\n\
expr        = or\n\
or          = and { \"||\" and }\n\
and         = cmp { \"&&\" cmp }\n\
cmp         = sum [ ( \"==\" | \"!=\" | \"<\" | \"<=\" | \">\" | \">=\" ) sum ]\n\
sum         = product { ( \"+\" | \"-\" ) product }\n\
product     = unary { ( \"*\" | \"/\" | \"%\" ) unary }\n\
unary       = [ \"!\" | \"-\" ] postfix\n\
postfix     = primary { \"[\" expr \"]\" | \".\" IDENT [ \"(\" [ expr { \",\" expr } ] \")\" ] }\n\
primary     = literal | interp | path [ \"(\" [ expr { \",\" expr } ] \")\" ]\n\
            | \"num\" \"(\" expr \")\" | \"str\" \"(\" expr \")\"\n\
            | struct_literal | \"$\" IDENT | \"(\" expr \")\"\n\
\n\
literal     = NUMBER | STRING | \"true\" | \"false\"\n\
interp      = 'f\"' { text | \"{\" expr \"}\" } '\"'\n\
path        = IDENT { \"::\" ( IDENT | keyword ) }\n\
block       = \"{\" { stmt } \"}\"\n\
\n\
# Precedence, loosest first, is Rust's: || && then one comparison, then + -,\n\
# then * / %, then unary ! -, then indexing and fields. Exactly one comparison:\n\
# comparisons do not chain.\n\
#\n\
# keywords: bool broadcast const costume else false fn for forever if in let list\n\
#   loop macro map match num on proc pub repeat repeat_until return sound sprite\n\
#   stage str struct true use var warp watch while\n\
# `for`, `in`, `loop`, `match` and `while` are reserved so the prelude can define\n\
# them, even though the parser produces them as ordinary statements.\n\
# punctuation: ( ) { } [ ] , ; : :: = -> => . .. ..= ! && || == != <= >= < > + - * / %\n\
#   the wildcard `_`, and `$` introducing a macro parameter\n";

/// `syntax` — the reference form of the syntax guide.
pub const SYNTAX: &str = "\n## syntax\n\
# # Lexical rules\n\
#\n\
#   comments     // to end of line, and /* block comments */ (they do not nest)\n\
#                `///` is a line comment like any other; no comment reaches the\n\
#                compiler\n\
#   identifier   [A-Za-z_][A-Za-z0-9_]*\n\
#   path         identifiers joined by `::`, e.g. std::motion or Key::Space. The\n\
#                segment AFTER `::` may be a keyword, because nothing there can be\n\
#                anything but an item: that is how events::broadcast and\n\
#                control::while are spelled\n\
#   number       10, -3.5, 1e3. The spelling is preserved exactly, so 1.50 stays\n\
#                1.50 in the project. A `-` directly before a numeric literal is\n\
#                part of the literal; anywhere else it is negation\n\
#   string       \"…\" with \\\\ \\\" \\n \\r \\t \\0 \\{ \\} and \\u{1F600}\n\
#   f-string     f\"…\" interpolates a braced expression in place; a doubled brace\n\
#                stands for a literal brace\n\
#   boolean      true, false\n\
#   whitespace   insignificant\n\
#   termination  items and block statements are not terminated; `let`, assignment,\n\
#                calls and `use` end with `;`\n\
#\n\
# # File, target, module\n\
#\n\
# A file is a target file (exactly one `stage` or `sprite`) or a module file (no\n\
# target). The stage is always named `Stage`; a sprite takes the name given and it\n\
# must be unique. See `modules`.\n\
#\n\
# # Declarations\n\
#\n\
#   var [decorators] [pub] IDENT: type = initializer ;\n\
#   const [pub] IDENT: type = literal ;\n\
#   struct [pub] IDENT { field: type, … }\n\
#   costume \"name\" = \"assets/x.svg\" [center \"32\" \"32\"] ;\n\
#   sound \"name\" = \"assets/x.wav\" ;\n\
#   broadcast \"name\" ;\n\
#   watch a, b ;\n\
#\n\
# `center X Y` moves the rotation centre; both are STRING literals, which is what\n\
# Scratch stores. Without it the centre is the middle of the image, worked out from\n\
# the file, and a file with no size is an error.\n\
#\n\
# `watch` names the cells that should be visible in the editor. For each it\n\
# declares a real Scratch variable whose monitor STARTS SHOWN, and every write to\n\
# the cell keeps it in step. It is the one place raven declares a Scratch variable,\n\
# and it exists to be looked at. A watched list needs no mirror: it already has a\n\
# monitor, so `watch` only shows it.\n\
#\n\
# A decorator answers exactly one question: where is the storage? No decorator is a\n\
# cell of `_vms`; `@scratch_global` is a real Scratch variable or list on the\n\
# stage; `@scratch_sprite` is one of the declaring sprite's own. Nothing else about\n\
# the name changes: the same type, the same scope rules, the same statements, and\n\
# `raven expand` prints data_variable and data_setvariableto where a cell would\n\
# print data_itemoflist. `watch` and the decorators compose: `watch` decides\n\
# whether the monitor starts visible, a decorator where the value is stored.\n\
#\n\
# A `const` is a literal with a name. It is substituted at every use and costs\n\
# nothing. It may be a pattern in a `match` arm.\n\
#\n\
# # Definitions\n\
#\n\
#   proc NAME (p: scalar, …) [-> type] [warp] { … }\n\
#   fn NAME (p: type, …) -> type { expr }\n\
#   macro NAME ($p: kind, …) -> type|stmts { expr | … }\n\
#\n\
# A `proc` becomes a real Scratch custom block, definition and mutation included.\n\
# Every parameter states its type and that type decides the reporter the body uses:\n\
#\n\
#   x: str   %s in the prototype, read with argument_reporter_string_number\n\
#   x: num   %n, read with argument_reporter_string_number\n\
#   x: bool  %b, read with argument_reporter_boolean\n\
#\n\
# `warp` marks the custom block run without screen refresh and is part of the\n\
# definition's mutation. With `-> type` the procedure has a result: `return e;`\n\
# stores e in the procedure's `_vms` cell and stops the block, and a call used as a\n\
# value reads that cell. A boolean result is read back as <cell = \"true\">, so a\n\
# call can be a condition.\n\
#\n\
# # Scripts and hats\n\
#\n\
#   on flag_clicked { }\n\
#   on key_pressed(Key::Space) { }\n\
#   on clicked { }\n\
#   on stage_clicked { }\n\
#   on clone_start { }\n\
#   on broadcast_received(\"reset\") { }\n\
#   on backdrop_switches_to(\"sky\") { }\n\
#   on greater_than(GreaterThan::Timer, 5) { }\n\
#\n\
# A hat is a Scratch hat block, named by the catalog with the `event_` prefix\n\
# dropped, and typed the way a std function is. `menus` lists every variant a\n\
# menu-taking hat accepts.\n\
#\n\
# # Statements\n\
#\n\
#   let x = e;               a _stackN cell outside a proc, a _vms cell inside one\n\
#   let x: num = 0;          the type is optional, and checked when written\n\
#   x = e;                   one cell write\n\
#   p.x = e;                 one cell write at the field's constant offset\n\
#   l[i] = e;                grow-and-replace\n\
#   x += e;  -=  *=  /=  %=  read, operator, write — three blocks\n\
#   return e;                only inside a proc that declares -> type\n\
#   if c { … } else { … }    `else if` is accepted and means else { if … }\n\
#   repeat n { … }\n\
#   repeat_until c { … }\n\
#   forever { … }            the same block as `loop`\n\
#   loop { … }\n\
#   while c { … }\n\
#   for i in a..b { … }      counts while i < b\n\
#   for i in a..=b { … }     counts while i <= b, so b runs too\n\
#   for x in items { … }     items is the list's NAME, not an expression\n\
#   match e { k => { … }, _ => { … } }\n\
#   call(args);              a method or a procedure used for its effect\n\
#\n\
# A pattern is a literal, a `const` name, or `_`. The `_` arm must be last and a\n\
# `match` needs at least one arm.\n\
#\n\
# `let` declares a NEW cell every time it runs, so a `let` in a loop\n\
# reinitializes each iteration and a `let` in a block disappears when the block\n\
# ends. If the name is already bound, the `let` shadows it for the rest of the\n\
# enclosing block. A `var` is never block-local: it is visible to every script in\n\
# its target, exactly as in Scratch.\n\
#\n\
# # Expressions\n\
#\n\
# Precedence is Rust's, and every operator maps to exactly one Scratch block:\n\
#\n\
#   a + b      operator_add            a - b     operator_subtract\n\
#   a * b      operator_multiply       a / b     operator_divide\n\
#   a % b      operator_mod            -a        operator_subtract(0, a)\n\
#   a == b     operator_equals         a != b    operator_equals + operator_not\n\
#   a < b      operator_lt             a > b     operator_gt\n\
#   a <= b     operator_lt + not       a >= b    operator_gt + not\n\
#   a && b     operator_and            a || b    operator_or\n\
#   !a         operator_not\n\
#   l[i]       data_itemoflist         l.at(i)   data_itemoflist\n\
#   l.first()  data_itemoflist         l.last()  data_itemoflist, index computed\n\
#   l.text()   data_listcontents       p.x       data_itemoflist, constant index\n\
#\n\
# `==` is Scratch's `=`: numeric when both sides look like numbers, and\n\
# case-insensitive text otherwise, so \"Apple\" == \"apple\" is true. `<` and `>`\n\
# compare numerically; non-numeric text compares as 0.\n\
#\n\
# Two of those rows cost two blocks: there is no not-equal, no <= and no >= in\n\
# Scratch. And `&&` / `||` are eager, so a guard like `i != 0 && 10 / i > 1`\n\
# divides by zero.\n\
#\n\
# A name written without its module is resolved against the whole standard library\n\
# when it is unambiguous: `motion::move_steps(10)` may be written `move_steps(10)`\n\
# as long as no other module's block has that name and nothing of yours does.\n";

/// `types` — the type system, the ownership model and purity.
pub const TYPES: &str = "\n## types\n\
# num, str, bool, list<num|str|bool>, map<num|str|bool, num|str|bool>, and\n\
# declared structs. There is no inference from use: a `var` states its type, a\n\
# `let` may state it or take it from the initializer.\n\
#\n\
# # The types\n\
#\n\
#   num        one cell   a number\n\
#   str        one cell   text\n\
#   bool       hexagonal  a yes/no answer; one cell, list item, map value or field\n\
#   list<T>    round      a run of cells\n\
#   map<K,V>   round      a run of alternating keys and values\n\
#   struct     not a value  a declared run of cells, one per scalar field\n\
#\n\
# Two conversions exist and both are free: num(x) and str(x), between num and str.\n\
# They change what the checker lets you do and nothing else: the target is untyped,\n\
# so neither emits a block, and they do not round, trim or parse — Scratch's own\n\
# coercion does that when the value reaches a block. There is no bool(x); write the\n\
# comparison you meant.\n\
#\n\
# # Booleans\n\
#\n\
# Scratch has no boolean literal, so `true` is lowered to <1 = 1> and `false` to\n\
# <1 = 0>, the one comparison with that constant value. A boolean INPUT is\n\
# hexagonal — wired to a block, never to a value — so a boolean read out of storage\n\
# is wrapped in a comparison:\n\
#\n\
#   var live: bool = false;   the arena's item is the boolean false\n\
#   live = 1 > 0;             replace item N of _vms with (1 > 0)\n\
#   if live { }               <item N of _vms = \"true\">\n\
#   if true { }               <1 = 1>\n\
#   f\"{live}\"                the same comparison, in a text slot\n\
#\n\
# <stored = \"true\"> is right for both shapes a stored boolean takes: a comparison's\n\
# own true/false, and the TEXT \"true\"/\"false\" a list loaded from project.json may\n\
# hand back. Cast.compare falls back to String(value) as soon as one side is not a\n\
# number, and String(true) is \"true\".\n\
#\n\
# # Shapes: where a value may go\n\
#\n\
# The block catalog records each input's Shape, and those records are the rule set:\n\
#\n\
#   Number, Positive, Whole, Integer, Angle   <- num\n\
#   Text                                      <- str and num\n\
#   Bool                                      <- bool only\n\
#   Color                                     <- \"#rrggbb\" or num, read as 0xrrggbb\n\
#   Broadcast                                 <- a declared broadcast name\n\
#   Menu(id)                                  <- the RavenType the menu table names\n\
#   Variable, List                            <- not expression slots; use the\n\
#                                                declaration, or pass the list value\n\
#\n\
# Because the catalog is the source of these rules, a new block arrives with its\n\
# types already enforced.\n\
#\n\
# # Values, containers and places\n\
#\n\
# num, str and bool are VALUES: copied, one cell each.\n\
# list<T> and map<K,V> are CONTAINERS: a run of cells reached through raven's\n\
# checked methods — xs.push(v), m.get(k) — never through a raw name.\n\
# A struct is a PLACE. It cannot be assigned, compared, returned or passed, `p.x`\n\
# reads one cell at a constant offset, and `seg.to.x` adds two compile-time offsets\n\
# into one constant. A struct field may be another struct.\n\
#\n\
# # Ownership\n\
#\n\
#   copyable   num, str, bool              assignment copies, passing copies\n\
#   owned      list, map, struct           cannot be assigned, passed or returned\n\
#\n\
# An owned value belongs to the declaration that named it, for the life of the\n\
# program. It is never copied, and it is never MOVED either — one step stricter\n\
# than Rust. `let a = b;` and `a = b;` are refused with \"which is not one value\",\n\
# and a list or map cannot be a proc parameter or result.\n\
#\n\
# The reason is the target: a complex value is a run of cells at a CONSTANT index,\n\
# which is what makes `p.x` one block and `xs[i]` an index the compiler computed,\n\
# and a Scratch custom block's parameters are scalars. A move would have to relocate\n\
# the run and pass a handle through a parameter the target does not have, turning\n\
# every access into a cell read. Forbidding the move buys the constant index back.\n\
# The rule is enforced where a value is read, not where it is declared. There is no\n\
# `&x`, no borrow and no lifetime, because a value that cannot be copied cannot be\n\
# lent.\n\
#\n\
# # Purity: pure, sampled, effectful\n\
#\n\
# Every block in the catalog carries one of three classes:\n\
#\n\
#   pure       same inputs, same result, always — operator_add, operator_join\n\
#   sampled    reads the world; two evaluations may differ — motion_xposition,\n\
#              sensing_timer, sensing_answer, operator_random\n\
#   effectful  a command; it changes something — motion_movesteps, data_addtolist\n\
#\n\
# A cell read is pure: a cell names a location, not a sensor. That is why a variable\n\
# used as a `match` subject needs no copy while `sensing::timer()` does. `m.get(k)`\n\
# is the one method that is not pure — it fills two cells of its own on the way — so\n\
# it is never duplicated.\n\
#\n\
# The class exists for one rule, and the rule exists because a macro parameter is a\n\
# substitution:\n\
#\n\
#   A macro parameter written into more than one statement of the expansion must be\n\
#   given an expression that may be evaluated more than once.\n\
#\n\
# Uses WITHIN one statement are always safe, because nothing runs between them,\n\
# which is why an `fn` may use a parameter as often as it likes. `let` is not\n\
# affected: `let x = motion::x_position();` reads the sensor once and every later\n\
# use of x reads the cell. Bind a value you want to sample once, and pass a pure\n\
# expression to a macro.\n\
#\n\
# # Locals\n\
#\n\
# A `let` is a cell: block-scoped, mutable and shadowing, one data_itemoflist per\n\
# read and one write per assignment. Outside a `proc` it is pushed on the script's\n\
# own `_stackN` and popped when the block ends; inside a `proc` it is a `_vms` cell\n\
# of the target, because a script's stack belongs to the script. A cell is a\n\
# location, not a stack frame, so a `let` inside a procedure is shared by every call\n\
# of it — which is why a recursive procedure may not have one.\n\
#\n\
# # Lists and maps\n\
#\n\
# List indexes are 1-based, as in Scratch, and are not checked at compile time. An\n\
# out-of-range read is \"\"; writing past the end GROWS the run to the index, so a\n\
# list is exactly as long as the highest index anything has written to it:\n\
#\n\
#   var xs: list<num> = [];\n\
#   xs[3] = 7;      // three items: \"\", \"\", 7\n\
#   xs[1] += 5;     // the first of them is now 5\n\
#\n\
# Reading past the end is still the empty string. Writing past the 200,000th item\n\
# cannot grow anything: Scratch refuses the block that would, and that is a limit of\n\
# the target, not of raven.\n\
#\n\
# A run that starts empty or is only ever written in place lives in `_vms`; a run\n\
# that is pushed or inserted into lives in `_heap`. Which is which is decided while\n\
# compiling.\n\
#\n\
# A map stores its entries as alternating keys and values, so its length is twice\n\
# its entry count. `get` on a missing key is \"\": the lookup returns 0 when the key\n\
# is absent and item 0 of a run is nothing, so the read is guarded rather than\n\
# trusting the lookup.\n\
#\n\
# # Monitors\n\
#\n\
# A watched value gets a monitor: the cell declares a real Scratch variable with\n\
# `visible` set and the project carries one monitor record for it. A watched value\n\
# with no position of its own is stacked down the left edge of the stage, one row\n\
# each, so two never sit on top of each other.\n";

/// `memory` — the arenas, cells, watch and the decorators.
pub const MEMORY: &str = "\n## memory\n\
# A raven program cannot name a Scratch variable. Every value it stores is a cell\n\
# of a Scratch list, addressed by a constant index chosen at compile time — unless\n\
# the declaration carries `@scratch_global` or `@scratch_sprite`, which store it in\n\
# a Scratch variable or list of its own name instead.\n\
#\n\
#   _vms      the one arena, declared on the stage: every `var` that is not\n\
#             Scratch's own, every `proc` frame, and every list or map that is only\n\
#             ever read or written in place. Declared with one item per cell, so a\n\
#             table costs nothing to start. Every target reads the same cells; no\n\
#             sprite has an arena of its own.\n\
#   _heap     the one heap, declared on the stage: the runs that grow. It is a list\n\
#             of its own because Scratch refuses to add to a list of 200,000 items,\n\
#             so a large table in _vms would stop every list from growing.\n\
#   _stackN   one per script that keeps block-scoped state, numbered across the whole\n\
#             project and declared on the stage. A `let` outside a proc pushes onto\n\
#             it and the matching data_deleteoflist pops when the block ends.\n\
#             Numbering starts at _stack1.\n\
#   _console  the log, declared only when something logs.\n\
#\n\
# None of them exists unless used.\n\
#\n\
# # What a statement becomes\n\
#\n\
#   reading a cell          data_itemoflist\n\
#   writing a cell          data_replaceitemoflist\n\
#   a `let` in a script     data_addtolist + data_deleteoflist, not a replace\n\
#   a proc frame            a cell of _vms, written before the call\n\
#   a struct field          a cell at a constant offset, one block to read\n\
#   a list or map run       a handle of (base, length, capacity) and the items\n\
#                           after it, read and written at a computed index\n\
#\n\
# # Where a Scratch variable can come from\n\
#\n\
#   watch a, b;                 a real Scratch variable per name, its monitor shown,\n\
#                               kept in step by every write to the cell. It is not a\n\
#                               name the program can look up.\n\
#   @scratch_global var x       data_variable(\"x\") / data_setvariableto by name on\n\
#                               the stage\n\
#   @scratch_sprite var x       one of the declaring sprite's own\n\
#   @scratch_global var xs: list<T>   the Scratch list itself\n\
#\n\
# No cell of an arena is laid out for any of those. `pub` decides who may name a\n\
# declaration; it never changes where the value lives.\n\
#\n\
# # What is refused\n\
#\n\
# The five blocks that name a Scratch variable rather than a list are not callable.\n\
# A program has no syntax for a variable name, so there is nothing for them to take.\n\
# `data_*` list blocks are refused too: a Scratch list block takes a list's NAME and\n\
# a raven list has no name to give it — `l.push(5)` is what exists.\n";

/// `costs` — the contract every construct keeps.
pub const COSTS: &str = "\n## costs\n\
# Blocks emitted per construct, beyond the code written inside it. These are the\n\
# same numbers `raven expand` shows; they are the contract, not an estimate.\n\
# `_stackN` is a per-script stack list, numbered across the project; `_vms` is the\n\
# one arena the stage declares and every target shares. A `let` outside a `proc` is\n\
# a `_stackN` push and a pop when its block ends; inside a `proc` it is a `_vms`\n\
# cell.\n\
\n\
stmt                 blocks   note\n\
x = e                1        _stackN for a let, _vms for a var or a pub var\n\
p.x = e              1        one cell at the field's constant offset\n\
l[i] = e             2 + the shared ensure helper (a call, then the replace)\n\
x += e               3        read, operator, write\n\
l.push(e)            1        a call to the shared push helper\n\
l.pop()              1        a call to the shared pop helper\n\
l.insert(i, e)       1        a call to the shared insert helper\n\
l.remove(i)          1        a call to the shared remove helper\n\
l.clear()            1        the length cell is set to zero\n\
m.set(k, v)          3 + the shared find helper\n\
m.remove(k)          3 + the shared find helper\n\
return e             2        arena cell write + control_stop(\"this script\")\n\
return;              1        control_stop(\"this script\")\n\
if / if-else         1\n\
repeat / repeat_until / forever  1\n\
loop                 1        control_forever, the same block as forever\n\
while c              2        operator_not + control_repeat_until\n\
for i in a..b        9 + cell counter on _stackN\n\
for i in a..=b       8 + cell counter on _stackN\n\
for x in items       13 + cell counter on _stackN, and one cell per element\n\
match e              1 per arm, plus 1 and a cell when the subject is sampled\n\
proc call            1\n\
reporter call        1 per block in the expression\n\
\n\
expr                 blocks   note\n\
a + b - * / %        1 each\n\
-a                   1\n\
a == b               1\n\
a != b, a <= b, a >= b  2   Scratch has no not-equal, no <= and no >=\n\
a && b, a || b       1        both sides evaluated, always\n\
!a                   1\n\
l[i], l.at(i), l.first()     1 for the first item, 2 at a computed index\n\
l.last()             3        item (base + length - 1) of the run\n\
l.text()             1        a call to the shared text helper\n\
l.is_empty(), m.is_empty()   2\n\
l.contains(v), l.index_of(v) 1 call to the shared find helper\n\
m.has(k)             1 + the shared find helper\n\
m.len()              2        the length over two\n\
m.get(k)             2 + the shared find helper (a guarded read)\n\
f\"…\"                 one operator_join per piece after the first\n\
true, false          1        no boolean literal: <1 = 1> / <1 = 0>\n\
a stored boolean read        1 extra   <cell = \"true\">\n\
num(x), str(x)       0        a retype, no block\n\
10, \"x\"             0        a literal input, not a block\n\
fn call              the body's blocks, inlined at the call site\n\
macro call           the expansion's blocks, inlined at the call site\n\
proc call with ->    the call, then a cell copy and a read (2)\n\
\n\
declaration          blocks   note\n\
var / pub var        0        a cell of _vms, declared with its starting value\n\
list / map           0        a run of _vms or _heap\n\
struct               0        one cell per field, from the literals\n\
const                0        substituted at every use\n\
costume / sound      0        packed into the archive\n\
broadcast            0        a name, once, on the stage\n\
watch                0 (1 per write to a watched cell, for the mirror)\n\
\n\
import               cost     note\n\
macro, fn, const     0        the definition, inlined at each call site\n\
proc                 1 custom block per target that imports it\n\
pub var              1 cell or run, once, for the whole project\n\
\n\
# The helper procedures above (__vm_*, __vh_*) are emitted once per target that\n\
# uses them, so a program with ten pushes still pays for one.\n";

/// `macros` — the one expansion mechanism.
pub const MACROS: &str = "\n## macros\n\
# raven has one expansion mechanism and no other. A macro is a named, typed,\n\
# hygienic, acyclic rewrite from syntax to syntax, and `while`, `for` and every\n\
# convenience a project adds are one. Four things are keywords instead, because\n\
# their shape is not a substitution: `f\"…\"` and `match` need repetition, compound\n\
# assignment has to know where its target lives, and `return` is a control-flow\n\
# statement.\n\
#\n\
#   macro        = \"macro\" IDENT \"(\" [ macro_params ] \")\" \"->\" result block\n\
#   macro_param  = \"$\" IDENT \":\" macro_kind\n\
#   macro_kind   = \"expr\" [ \"<\" type \" >\" ] | \"ident\" | \"block\"\n\
#   result       = type | \"stmts\"\n\
#\n\
#   pub macro concat($a: expr<str>, $b: expr<str>) -> str {\n\
#       operators::join($a, $b)\n\
#   }\n\
#\n\
#   pub macro count_up($times: expr<num>, $body: block) -> stmts {\n\
#       let counter = 0;\n\
#       repeat $times { $body; counter += 1; }\n\
#   }\n\
#\n\
#   count_up(3) { looks::say(\"hi\"); }\n\
#\n\
# A macro that takes a block parameter is called with the block AFTER the call, the\n\
# way a body-taking block is. A macro whose result is a type may be used anywhere an\n\
# expression is; one whose result is `stmts` is a statement.\n\
#\n\
# # Parameters\n\
#\n\
#   $x: expr        any expression; substituted wherever $x appears\n\
#   $x: expr<num>   an expression of that type, checked first\n\
#   $x: ident       a name; substituted and resolved AT THE CALL SITE\n\
#   $x: block       a { … } statement block, substituted as statements\n\
#\n\
# The `$` is written where a parameter is DECLARED and where its value is USED. In\n\
# between, a block parameter stands on its own line as `$body;`.\n\
#\n\
# # fn is a macro\n\
#\n\
# An `fn` is `macro` with expression parameters and an expression result:\n\
#\n\
#   fn hypot(a: num, b: num) -> num { operators::mathop(MathOp::Sqrt, a * a + b * b) }\n\
#\n\
# is precisely\n\
#\n\
#   macro hypot($a: expr<num>, $b: expr<num>) -> num {\n\
#       operators::mathop(MathOp::Sqrt, $a * $a + $b * $b)\n\
#   }\n\
#\n\
# `fn` parameters are written the way parameters usually are — `a: num`, used in the\n\
# body as `a` — and the compiler rewrites those uses into parameters before the\n\
# expander runs. That rewrite is the whole of the sugar. An `fn` is inlined at every\n\
# call site, so `let h = hypot(3, 4);` emits the body's blocks where the call was and\n\
# nothing else: no custom block, no call, no return cell.\n\
# An `fn` may not contain statements. That is not stylistic: it is the consequence of\n\
# being a substitution, and it is why an `fn` cannot loop, branch or recurse. If you\n\
# need any of those, or a value that has to come back from a real Scratch custom\n\
# block, write a `proc` with a result type.\n\
#\n\
# # Hygiene\n\
#\n\
# Names the macro INTRODUCES are the macro's own: every expansion renames them, so\n\
# they cannot collide with the caller's names. The generated cell is real — the\n\
# script's `_stackN` cell, or a `_vms` cell inside a `proc` — and it appears in\n\
# `raven expand`, but it is addressed by index and cannot appear in the editor's\n\
# variable list. There is no variable list to appear in. Cells are handed out in\n\
# source order, so they are stable across builds.\n\
# Names PASSED IN resolve at the call site: a macro taking `$x: ident` and writing\n\
# `$x = 0;` assigns the cell the caller's `x` names. A macro may declare a `var`\n\
# under a name the caller supplied, which is how `for` gives you the counter you\n\
# asked for:\n\
#\n\
#   pub macro for_range($i: ident, $from: expr<num>, $to: expr<num>, $body: block) -> stmts {\n\
#       let $i: num = $from;\n\
#       repeat_until $i >= $to { $body; $i += 1; }\n\
#   }\n\
#\n\
# Because `$i` is an `ident` parameter it is not renamed, so the binding is the name\n\
# the caller wrote, in the caller's block.\n\
#\n\
# # Totality\n\
#\n\
# A parameter is textually substituted — copy, not call. A macro body may call other\n\
# macros; the call graph over DEFINITIONS must be acyclic; a cycle is reported at\n\
# every call of a macro whose definition reaches itself, and the note names the whole\n\
# chain. Expansion stops after 32 levels, and a program that deep is an error too.\n\
# There is no conditional expansion and no compile-time evaluation, so the set of\n\
# macros a program uses is decidable by looking at it.\n\
# A `for` inside a `for`, or a `while` inside a `while`, is an ordinary program. What\n\
# is refused is a macro whose own definition calls it:\n\
#\n\
#   macro again() -> stmts { again(); }   // error: expands into itself\n\
#\n\
# The distinction is the definition. A call written in a macro body is a step of that\n\
# expansion; the same name arriving through a substituted `$body` belongs to the\n\
# caller, who wrote it once and gets it expanded once.\n\
#\n\
# # The prelude\n\
#\n\
# std::prelude is imported into every file. It is an ordinary raven module —\n\
# crates/raven/src/prelude.rav, printed verbatim by `raven explain prelude` — so\n\
# every item in it is a macro a program can read or shadow. A program's definition\n\
# wins over the prelude's.\n\
#\n\
#   loop { … }                  forever { … }                              nothing\n\
#   while c { … }               repeat_until !c { … }                      2 blocks\n\
#   for i in a..b { … }         a _stackN cell, then repeat_until           9 blocks\n\
#   for i in a..=b { … }        the same, testing > instead of >=           8 blocks\n\
#   for x in items { … }        a counter, and a cell per element           13 blocks\n\
#   abs floor ceil sqrt ln log10 exp pow10   operators::mathop with the menu entry\n\
#\n\
# The trigonometric shorthands are deliberately absent: Scratch measures angles in\n\
# degrees, so `sin(x)` would read as radians and be wrong. Write\n\
# `operators::mathop(MathOp::Sin, x)` for those.\n\
#\n\
# # Errors in expanded code\n\
#\n\
# An error inside generated code reports the line YOU wrote and adds the definition\n\
# of the macro as a note, never pointing at generated text:\n\
#\n\
#   error: `count_up` expects `num` for `$times`, found `str`\n\
#     --> src/sprites/player.rav:14:9\n\
#      = note: it is declared as count_up($times: expr<num>, $body: block) -> stmts\n\
#      = note: it is defined in src/lib/loops.rav\n";

/// `modules` — files, `use`, visibility and what an import costs.
pub const MODULES: &str = "\n## modules\n\
# A module is a file. There are no inline `mod` blocks, no `mod.rs` and no path\n\
# attributes: the path IS the file path, relative to `src/`, with `/` written as\n\
# `::`.\n\
#\n\
#   src/stage.rav                  the stage target\n\
#   src/sprites/player.rav         a sprite target\n\
#   src/lib/geometry.rav           module `lib::geometry`\n\
#   src/lib/text/wrap.rav          module `lib::text::wrap`\n\
#\n\
# # Two kinds of file\n\
#\n\
# A TARGET file declares exactly one `stage` or `sprite` and holds its code.\n\
# A MODULE file declares no target: it holds macros, `fn`s, `const`s, `proc`s and\n\
# project-wide state, and other files import it.\n\
# A module has no target, so it may declare no sprite-local Scratch variable. A\n\
# `pub var` is the state it can declare, and that declaration is a cell of the\n\
# project's one arena. A sound, a non-`pub` `var`, or a `@scratch_sprite` var in a\n\
# module file is an error.\n\
# A COSTUME is the one thing a module declares that a target really wears: every\n\
# target that uses the module gets its costumes, AFTER the target's own. A name the\n\
# target declares itself wins.\n\
#\n\
# # use\n\
#\n\
#   use lib::geometry;                    the module\n\
#   use lib::geometry::hypot;             one item from it\n\
#   use lib::geometry::{hypot, square};   two items from it\n\
#   use std::pen;                         an intrinsic module needs no import\n\
#\n\
# A path names a module, or a module and one item in it, and the two are told apart\n\
# by looking: if src/lib/geometry.rav exists and src/lib/geometry/hypot.rav does\n\
# not, then lib::geometry::hypot is the module lib::geometry and the item hypot.\n\
# Paths are resolved from src/. The prelude is imported into every file without\n\
# being written.\n\
# Importing a module puts ALL of its public items in scope by name; naming items\n\
# does not restrict that, it documents it, and each name is checked to exist and be\n\
# `pub`. There is no `geometry::hypot` qualifier: a name is either in scope or it is\n\
# not.\n\
#\n\
# # What an import costs\n\
#\n\
#   macro      the definition, inlined at each call site      nothing, ever\n\
#   fn         the definition, inlined at each call site      nothing, ever\n\
#   const      the literal                                    nothing, ever\n\
#   proc       a copy of the custom block, IN THAT TARGET     one per importing target\n\
#   pub var    the shared cell or run of the one arena        one cell or run, once\n\
#\n\
# A Scratch custom block belongs to exactly one target, so raven copies it — as\n\
# raven-asm's `use` does, and as the editor itself does when a sprite is duplicated.\n\
# `raven expand` shows the copies.\n\
#\n\
#   Put logic that should be shared and free in a `macro` or an `fn`. Put logic that\n\
#   must be a real custom block in a `proc`, and expect a copy per sprite.\n\
#\n\
# # Visibility\n\
#\n\
#   pub        importable by other files. It never changes where a value lives:\n\
#              every var is a cell or run of the one arena whether or not it is pub.\n\
#   no pub     visible only inside the declaring file. In a module file, a non-pub\n\
#              `var` is an error: a module has no target for it to belong to.\n\
#\n\
# Importing a non-`pub` item is an error naming the module and the missing `pub`.\n\
# A `pub struct` parses but cannot travel: a struct is resolved inside the target\n\
# that declares it. There is no `pub(crate)` and no nesting of visibility.\n\
#\n\
# # Cycles\n\
#\n\
# Module imports must be acyclic, and a cycle is reported with the full path of the\n\
# import chain rather than the first repeated name. Macro expansion has its own\n\
# acyclicity requirement and is checked separately: a module graph can be a DAG\n\
# while its macros are not, and vice versa.\n\
#\n\
# # Project-wide state\n\
#\n\
# A `pub var` declared anywhere is one cell of the project's single arena, so it has\n\
# exactly one instance and every file that imports it reads and writes that one\n\
# cell. A `var` that is not `pub` has one instance too, and only its own file can\n\
# name it. Because the variable is one shared cell there is no ambiguity about whose\n\
# copy is being read, which is why visibility and storage are both written down\n\
# rather than inferred from use.\n";

/// `errors` — the diagnostic format and the classes of error.
pub const ERRORS: &str = "\n## errors\n\
# # The format\n\
#\n\
# Line-oriented, so it reads the same in a terminal, in an editor and in a CI log:\n\
#\n\
#   error: `score` must be `num`, found `str`\n\
#     --> src/sprites/player.rav:18:23\n\
#      |\n\
#   18 |                 score += 1;\n\
#      |                       ^^^^^\n\
#      = note: convert it with `num()`; the two directions Scratch coerces are free\n\
#\n\
# The caret always points at a line the author wrote. An error inside generated code\n\
# reports the line YOU wrote and adds what the macro was and where it is defined:\n\
#\n\
#      = note: it is declared as count_up($times: expr<num>, $body: block) -> stmts\n\
#      = note: it is defined in src/lib/shapes.rav:32:12\n\
#\n\
# An unresolved name lists the closest matches when there are any. Exit codes: 0\n\
# success, 1 a diagnostic, 2 bad command line usage. A panic with a Rust backtrace\n\
# is always a bug in the compiler.\n\
#\n\
# # Classes, and what to write instead\n\
#\n\
# syntax\n\
#   expected a condition, found true          a hexagonal input wants a block, not a\n\
#                                             value: write `if live { }`, or `1 = 1`\n\
#   comparisons do not chain                  a < b < c: write a < b && b < c\n\
#   expected `;`                              assignment, `let`, a call and `use` are\n\
#                                             terminated; blocks are not\n\
#\n\
# names\n\
#   unresolved name `foo`, did you mean `bar`  the spelling is not in `stdlib` and\n\
#                                             not declared in the project\n\
#   `foo` is not `pub`                        the item exists in another file and is\n\
#                                             not importable\n\
#   a `for` over a name that is not a list    `for x in items` takes the list's name\n\
#\n\
# types\n\
#   `x` must be `num`, found `str`            num(x) and str(x) are the only\n\
#                                             conversions, and both are free\n\
#   which is not one value                    a list, map or struct cannot be\n\
#                                             assigned, passed or returned\n\
#   `if` needs a condition, found `num`       `if` takes a bool; write `if n != 0`\n\
#\n\
# ownership and storage\n\
#   a struct is a place, not a value           build it in a `let`/`var` initializer\n\
#                                             and write its fields\n\
#   a list cannot be a parameter or a result   pass the run's name is not possible:\n\
#                                             keep the run in a `var` the proc names\n\
#   a `let` inside a recursive procedure       pass the value as a parameter\n\
#   this struct is over 64 cells               a struct is a fixed frame\n\
#\n\
# macros\n\
#   `m` expects `num` for `$x`, found `str`   the note prints the declaration\n\
#   `$x` is evaluated twice                   bind it with `let` first, or pass a\n\
#                                             pure expression\n\
#   `m` expands into itself                   a macro body may not reach itself\n\
#   expansion is 32 levels deep               the bound is what keeps the compiler\n\
#                                             from running out of stack first\n\
#\n\
# shapes\n\
#   expected a value, found a name            a menu that is not acceptReporters\n\
#                                             takes a variant, not an expression\n\
#   expected a name, found a value            `data_*` takes a declared name; use the\n\
#                                             declaration\n\
#\n\
# refused blocks\n\
#   the five data_* blocks that name a variable, and the data_* list blocks, are\n\
#   refused with the raven spelling to write instead (`memory` has the list)\n\
#\n\
# runtime\n\
#   `Main` declares no costumes              every target needs at least one costume\n\
#   a file with no size                       a costume file the asset loader cannot\n\
#                                             read a size from\n";
