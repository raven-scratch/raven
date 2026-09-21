# Syntax

The whole grammar fits on a few pages, and it is closer to raven-asm than it looks:
raven keeps Scratch's vocabulary and adds only the notation a text file needs.

Everything here is core language. The fixed-shape conveniences — `while` and
`for` — are macros from the [prelude](/raven/std#prelude), and are described in
[macros](/raven/macros). `f"…"`, `match`, compound assignment and `return` are
keywords whose lowerings are written down in
[the cost table](/raven/lowering#the-cost-table).

## Lexical rules

* **Comments** — `// to end of line` and `/* block comments */` (they do not
  nest). There are no doc comments: `///` is a line comment like any other, and
  nothing a comment says reaches the compiler.
* **Identifiers** — `[A-Za-z_][A-Za-z0-9_]*`.
* **Paths** — identifiers joined by `::`, for example `std::motion` or
  `Key::Space`. The segment *after* `::` may be a keyword, because nothing there
  can be anything but an item: that is how `events::broadcast` and
  `control::while` are spelled.
* **Numbers** — `10`, `-3.5`, `1e3`. The spelling is preserved exactly, so `1.50`
  stays `1.50` in the project. A `-` directly before a numeric literal is part of
  the literal; anywhere else it is negation.
* **Strings** — `"…"` with `\\`, `\"`, `\n`, `\r`, `\t`, `\0`, `\{`, `\}` and
  `\u{1F600}`. An `f"…"` string interpolates a braced expression in place, and a
  doubled brace stands for a literal brace.
* **Booleans** — `true`, `false`.
* **Keywords** — `bool` `broadcast` `const` `costume` `else` `false` `fn` `for`
  `forever` `if` `in` `let` `list` `loop` `macro` `map` `match` `num` `on` `proc`
  `pub` `repeat` `repeat_until` `return` `sound` `sprite` `stage` `str` `struct`
  `true` `use` `var` `warp` `watch` `while`. `for`, `in`, `loop`, `match` and
  `while` are reserved so the prelude can define them.
* **Punctuation** — `( ) { } [ ] , ; : :: = -> => . .. ..= ! && || == != <= >= < > + - * / %`
  the wildcard `_`, and `$` introducing a macro parameter.
* **Whitespace** — insignificant. Items and block statements are not terminated;
  `let`, assignment, calls and `use` end with `;`.

## File

```text
file      = { use } { item }
use       = "use" path [ "::" "{" ident { "," ident } "}" ] ";"
```

A `.rav` file is one of two things:

* a **target file**, which declares exactly one `stage` or `sprite` and holds its
  code;
* a **module file**, which declares no target and holds items other files import.

The same distinction raven-asm makes, for the same reason: a Scratch custom block
belongs to a target, so there has to be a file that owns one. See
[modules](/raven/modules).

## Target

```text
target    = "stage" block
          | "sprite" STRING block
```

The stage is always named `Stage`. A sprite takes the name you give it, and the
name must be unique among the targets: `sprite "Stage"` and a second sprite with
an existing name are both errors.

## Declarations

```text
var_decl    = [ "pub" ] "var" ( IDENT | "$" IDENT ) ":" type "=" initializer ";"
initializer = literal | "[" [ literal { "," literal } ] "]" | struct_initializer
struct_decl = [ "pub" ] "struct" IDENT "{" { IDENT ":" type [ "," ] } "}"
const_decl  = [ "pub" ] "const" IDENT ":" type "=" literal ";"
broadcast   = "broadcast" STRING ";"
costume     = "costume" STRING "=" STRING [ "center" STRING STRING ] ";"
// center X Y moves the rotation centre. Both are STRING literals, which is what
// Scratch stores. Without it, the centre is the middle of the image, worked out
// from the file, and a file with no size is an error.
sound       = "sound" STRING "=" STRING ";"

type        = "num" | "str" | "bool" | "list" "<" scalar ">" | "map" "<" scalar "," scalar ">" | IDENT
scalar      = "num" | "str" | "bool"
```

```rav
// src/sprites/player.rav
sprite "Player" {
    costume "idle" = "assets/idle.svg" center "32" "32";
    sound "beep" = "assets/beep.wav";

    var score: num = 0;
    var trail: list<num> = [];
    var totals: map<str, num> = [];
    var home: Point = Point { x: 0, y: 0 };
    pub var best: num = 0;

    const MAX_SPEED: num = 12;

    broadcast "reset";
}

struct Point { x: num, y: num }
```

`list<T>` and `map<K, V>` are containers; a struct is a *place*, so a struct
initializer is only written where a place is being made — the initializer of a
`let` or a `var`. See [types](/raven/types#structs).

| Written | Scratch keeps | Notes |
| --- | --- | --- |
| `num` | a number | The spelling you wrote is preserved. |
| `str` | a string | |
| `bool` | `true` / `false` | Only as a variable's initial value; boolean **inputs** want a boolean expression. |
| `list<T>` | a list, declared empty or with literal items | |

`pub` puts the declaration on the stage, where every sprite can see it. Without
it the declaration belongs to the file's target — a sprite-local variable, or, in
the stage file, a project-wide one. A module file has no target, so everything it
exports is `pub` by construction.

`const` is a literal with a name. It is substituted at every use and costs
nothing; it cannot be computed, because raven has no compile-time evaluation
([law 4](/raven/design#_4-expansion-is-total)).

### `watch`

```text
watch_decl = "watch" IDENT { "," IDENT } ";"
```

A raven `var` is a cell of the virtual memory system, so the editor has nothing
to show for it. `watch` names the ones that should be visible, and for each it
declares a real Scratch variable — whose monitor **starts shown** — that every
write to the cell keeps in step. It is the one place raven declares a Scratch
variable, and it exists to be looked at, not programmed with.

```rav
sprite "Player" {
    var score: num = 0;
    var trail: list<num> = [];
    watch score, trail;        // both monitors appear on the stage

    on flag_clicked { score += 1; }   // the monitor follows, for one block more
}
```

A watched **list** needs no mirror: it already has a monitor, so `watch` only
shows it. Anything watched has to be declared in the target that watches it (or
be a `pub var`), which the checker enforces.

## Definitions

```text
proc      = "proc" IDENT "(" [ params ] ")" [ "->" scalar ] [ "warp" ] block
params    = param { "," param }
param     = IDENT ":" scalar

fn        = "fn" IDENT "(" [ fn_params ] ")" "->" type expr_block
fn_params = fn_param { "," fn_param }
fn_param  = IDENT ":" type

macro        = "macro" IDENT "(" [ macro_params ] ")" "->" macro_result ( expr_block | block )
macro_params = macro_param { "," macro_param }
macro_param  = "$" IDENT ":" macro_kind
macro_kind   = "expr" [ "<" type ">" ] | "ident" | "block"
macro_result = type | "stmts"

expr_block = "{" expr "}"
```

```rav
proc zigzag(degrees: num, steps: num) warp {
    motion::turn_right(degrees);
    motion::move_steps(steps);
}

proc clamp(value: num, low: num, high: num) -> num {
    if value < low { return low; }
    if value > high { return high; }
    return value;
}

fn hypot(a: num, b: num) -> num {
    operators::mathop(MathOp::Sqrt, a * a + b * b)
}

macro stop_all() -> stmts {
    control::stop(StopOption::All);
}
```

* A **`proc`** becomes a real Scratch custom block, definition and mutation
  included. Every parameter states its type, and the reporter that reads it is
  checked against that type. With `-> type` it also has a result: `return e;`
  stores `e` in the procedure's cell in [`_vms`](/raven/design#the-virtual-memory-system)
  and stops the custom block, and a call used as a value reads that cell.
* An **`fn`** is a named expression. Its parameters are named like parameters and
  used like names; it is inlined at every call site, may not contain statements,
  and therefore cannot loop, branch or recurse.
* A **`macro`** is a named rewrite. It may take expressions, names and blocks, and
  it produces either a value or statements. See [macros](/raven/macros).

## Scripts

```text
script    = "on" hat block
hat       = IDENT [ "(" [ expr { "," expr } ] ")" ]
```

A hat is a Scratch hat block, named by the catalog with the `event_` prefix
dropped, and typed the same way a std function is:

```rav
on flag_clicked { }
on key_pressed(Key::Space) { }
on clicked { }
on stage_clicked { }
on clone_start { }
on broadcast_received("reset") { }
on backdrop_switches_to("sky") { }
on greater_than(GreaterThan::Timer, 5) { }
```

There is **no** top-level statement. Every statement lives inside an `on` script
or a `proc`, because a loose stack in Scratch never runs.

## Statements

```text
stmt = let         | assign      | op_assign   | return     | if_stmt
     | repeat_stmt | repeat_until| forever     | loop_stmt  | while_stmt
     | for_stmt    | match_stmt  | var_decl    | call ";"

let          = "let" IDENT [ ":" type ] "=" expr ";"
assign       = lvalue "=" expr ";"
lvalue       = ( IDENT | "$" IDENT ) | index | field
index        = IDENT "[" expr "]"
field        = IDENT { "." IDENT }
op_assign    = lvalue ( "+=" | "-=" | "*=" | "/=" | "%=" ) expr ";"
return       = "return" [ expr ] ";"
if_stmt      = "if" expr block [ "else" ( if_stmt | block ) ]
repeat_stmt  = "repeat" expr block
repeat_until = "repeat_until" expr block
forever      = "forever" block
loop_stmt    = "loop" block
while_stmt   = "while" expr block
for_stmt     = "for" IDENT "in" ( expr ".." expr | expr "..=" expr | expr ) block
match_stmt   = "match" expr "{" { pattern "=>" block } "}"
pattern      = literal | IDENT | "_"
```

A statement is also a **method call used for its effect** — `trail.push(v);`,
`totals.set(k, v);` — which is the `call ";"` form above. Which methods are
statements and which produce a value is a property of the receiver's type; see
[the standard library](/raven/std#prelude).

The `"$" IDENT` form of `lvalue` only appears inside a macro body; see
[macros](/raven/macros).

```rav
let total = x * 2 + 1;          // a `_stack1` cell, scoped to this block
let best: num = 0;              // the type is optional, and checked when written
score = 0;                      // a declared variable: one cell write
trail[1] = 7;                   // a declared list
home.x = 8;                     // a struct field: one cell at a constant offset
trail.push(9);                  // a list method used for its effect
score += 1;                     // a cell read, an add and a cell write
total += 1;                     // the same three blocks, on another cell
return value;                   // only inside a proc that declares -> type
repeat 10 { motion::move_steps(1); }
repeat_until sensing::key_pressed(Key::Space) { }
forever { }
loop { }                            // the same block, spelled the other way
while score < 10 { score += 1; }
for i in 1..10 { looks::say(f"{i}"); }
for i in 1..=10 { looks::say(f"{i}"); }   // the last step runs too
for x in trail { looks::say(x); }         // the list, element by element
match score { 0 => { looks::say("zero"); }, _ => { } }
match STEP { SIDES => { looks::say("a square"); }, _ => { } }   // a const is a pattern
```

A pattern is a literal, a `const` name — which is a literal with a name — or `_`.
The `_` arm must be last, and a `match` needs at least one arm.

| Statement | Core? | Lowers to |
| --- | --- | --- |
| `let x = e` | core | a `data_addtolist` onto the script's `_stackN`, and a `data_deleteoflist` when the block ends; each read is a `data_itemoflist` |
| `let p: Point = Point { … }` | core | one cell push per field, into a fresh frame |
| `x = e` | core | one cell write: the script's `_stackN` for a `let`, `_vms` for a `var`, `_gvm` for a `pub var` |
| `p.x = e` | core | one cell write at the field's constant offset |
| `l[i] = e` | core | a grow-and-replace: the `replace item` block, plus up to five to lengthen the list first |
| `x += e` | core | a read, the operator and a write — three blocks, wherever `x` lives |
| `other op=` | core | the same three blocks with the matching operator |
| `l.push(e)`, `l.insert(i, e)`, `l.remove(i)`, `l.clear()`, `l.pop()` | core | the matching list block, through raven's checked list |
| `return e;` | core | a `_vms` cell write and `control_stop` |
| `return;` | core | `control_stop` |
| `if` / `else` | core | `control_if` / `control_if_else` |
| `repeat` | core | `control_repeat` |
| `repeat_until` | core | `control_repeat_until` |
| `forever` | core | `control_forever` |
| `while` | macro | `operator_not` and `control_repeat_until` — two blocks |
| `loop` | macro | `control_forever`, exactly as `forever` |
| `for i in a..b` | macro | a `_stackN` cell, `data_addtolist` and `repeat_until` |
| `for i in a..=b` | macro | the same, with `>` where `..` uses `>=` |
| `for x in items` | macro | the counter of `..`, plus a `_stackN` cell per element |
| `match` | core | a chain of `control_if_else` |
| `f(x)` where `f` returns a value | core | the `procedures_call`, one cell copy, and a `data_itemoflist` |
| `control::while(c) { }` | std | `control_while` — the extended block, if you want it |
| `control::for_each(i, n) { }` | std | `control_for_each`, counting a variable you already have |

`else if` is accepted and means `else { if … }`.

`for` has three shapes, and they are three macros:

* `for i in a..b { … }` counts from `a` while `i < b`;
* `for i in a..=b { … }` counts from `a` while `i <= b`, so `b` runs too;
* `for x in items { … }` binds `x` to each element of a list, in order. `items` is
  the list's **name** — a `var` or a `let`, not an expression — because the loop
  reads its length each turn; `items.at(2)` is not a list to walk. The element is
  a `let` in the loop's own block, so it is gone when the loop ends.

A loop may sit inside a loop of the same kind: `for` inside `for`, `while` inside
`while`. Only a macro whose own definition calls it is a cycle.

`let` declares a **new** cell every time it runs, so a `let` inside a loop
reinitializes its cell each iteration, and a `let` inside a block disappears when
the block ends. If a name is already bound, the `let` shadows it for the rest of
the enclosing block. To keep a value across statements of a target, or to share it
with another script, declare a `var` — a cell that outlives every script, at
target scope (`_vms`) or project scope (`pub`, in `_gvm`).

A `var` statement is allowed **only inside a macro body**, where it declares a
cell for that expansion, or a name the caller passed as an `ident` parameter.
Anywhere else it is an error: a `var` belongs to a target, so it is declared
beside the other `var`s.

A method call is a statement when the method is one (`l.push(v);`) and an
expression when it produces a value (`l.len()`). Which methods exist depends on
the receiver's type; see [the standard library](/raven/std#prelude).

There is no `break` and no `continue`; Scratch cannot express them. See
[design laws](/raven/design#break-and-continue).

## Expressions

```text
expr    = or
or      = and { "||" and }
and     = cmp { "&&" cmp }
cmp     = sum [ ( "==" | "!=" | "<" | "<=" | ">" | ">=" ) sum ]
sum     = product { ( "+" | "-" ) product }
product = unary { ( "*" | "/" | "%" ) unary }
unary   = [ "!" | "-" ] postfix
postfix = primary { "[" expr "]" | "." IDENT [ "(" [ expr { "," expr } ] ")" ] }
primary = literal | interp | path [ "(" [ expr { "," expr } ] ")" ]
        | "num" "(" expr ")" | "str" "(" expr ")"
        | struct_literal | "$" IDENT | "(" expr ")"
```

Precedence is Rust's, and every operator maps to exactly one Scratch block:

| Written | Blocks | Notes |
| --- | --- | --- |
| `a + b` | `operator_add` | |
| `a - b` | `operator_subtract` | |
| `a * b` | `operator_multiply` | |
| `a / b` | `operator_divide` | |
| `a % b` | `operator_mod` | |
| `-a` | `operator_subtract(0, a)` | |
| `a == b` | `operator_equals` | Scratch compares numbers numerically and everything else **case-insensitively**. |
| `a != b` | `operator_equals` + `operator_not` | Two blocks: Scratch has no `≠`. |
| `a < b` | `operator_lt` | |
| `a > b` | `operator_gt` | |
| `a <= b` | `operator_lt` + `operator_not` | Two blocks: Scratch has no `≤`. |
| `a >= b` | `operator_gt` + `operator_not` | Two blocks. |
| `a && b` | `operator_and` | **Both sides are evaluated.** |
| `a \|\| b` | `operator_or` | **Both sides are evaluated.** |
| `!a` | `operator_not` | |
| `l[i]`, `l.at(i)` | `data_itemoflist` | |
| `l.first()`, `l.last()` | `data_itemoflist` | `last` computes the index at run time |
| `l.text()` | `data_listcontents` | the whole list, one block |
| `m.get(k)`, `m.has(k)` | `data_itemnumoflist` plus a read or a comparison | |
| `p.x` | `data_itemoflist` | one cell, at a constant offset | |

Two of those rows are the kind of thing this documentation exists to say out
loud: `!=`, `<=` and `>=` cost two blocks, and `&&`/`||` are eager. A guard like
`i != 0 && 10 / i > 1` divides by zero.

A comparison does not chain: `a < b < c` is an error, not `(a < b) < c`. Write
`a < b && b < c`, and remember that both operands are evaluated.

`==` is Scratch's `=`: numeric if both sides look like numbers, and
case-insensitive string comparison otherwise. raven does not paper over it.

A call may name a `proc` that declares a `-> num`, `-> str` or `-> bool` result;
the call is hoisted one statement so the value can be read, and that shows up in
`raven expand`. A boolean that comes back is read as `<cell = "true">`, so the
call can be a condition. See
[booleans](/raven/types#booleans-stored-as-a-value-converted-on-the-way-out). A name
written without a module is resolved against the whole standard library when it is
unambiguous — `motion::move_steps(10)` may be written `move_steps(10)`, as long as
no other module's block has that name and nothing of yours does.

## What is deliberately absent

```rav
if c { 1 } else { 2 }      // no conditional expression: use an if statement
break;                     // Scratch cannot leave a loop without ending the script
var y: num = 0;            // inside a proc: use `let`, or declare it on the target
a as num                   // no as: write num(a)
#[warp]                    // no attributes: write proc … warp
"score: " + score          // no string +: write f"score: {score}"
```
