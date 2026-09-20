# Macros

raven has one expansion mechanism and no other. A macro is a named, typed,
hygienic, acyclic rewrite from syntax to syntax, and `while`, `for` and every
convenience a project adds are one. Four things are keywords instead, because
their shape is not a substitution: `f"…"` and `match` need *repetition*,
compound assignment has to know where its target lives — a cell, or a list
element — and `return` is a control-flow statement. All four have a written
lowering in [the cost table](/raven/lowering#the-cost-table) and are on the
[design laws](/raven/design#_3-sugar-is-a-macro-or-it-is-a-keyword-with-a-written-lowering)
page.

## Why one mechanism

Scrust grew its sugar in the compiler. `match` became `if`/`else` in one function,
`join` folded in another, package imports were renamed in a third, and `let`
became a list-allocator call in a fourth. Each was reasonable alone; together they
meant no one could say what a construct cost, and a bug in any one of them showed
up as a wrong `.sb3` with no source line to blame.

raven puts all of it in one place, with four properties that make it safe:

* **Typed** — a macro declares what each parameter is (`expr`, `ident`, `block`)
  and what it produces (`expr`, `stmts`), so an ill-formed expansion is a
  compile-time error and not a surprise.
* **Hygienic** — a name the macro introduces refers to the macro's own name, not
  the caller's, however the caller's scopes are named.
* **Total** — expansions form a DAG. There is no recursion and no compile-time
  evaluation, so expansion always terminates.
* **Printable** — `raven expand` shows the result.

## Declaring a macro

```text
macro        = "macro" IDENT "(" [ macro_params ] ")" "->" result block
macro_param  = "$" IDENT ":" macro_kind
macro_kind   = "expr" [ "<" type ">" ] | "ident" | "block"
result       = type | "stmts"
```

```rav
/// Join two pieces of text.
pub macro concat($a: expr<str>, $b: expr<str>) -> str {
    operators::join($a, $b)
}

/// Run a body a fixed number of times.
pub macro count_up($times: expr<num>, $body: block) -> stmts {
    let counter = 0;
    repeat $times {
        $body;
        counter += 1;
    }
}
```

| Parameter kind | Accepts | Substituted as |
| --- | --- | --- |
| `$x: expr` | any expression | the expression, wherever `$x` appears |
| `$x: expr<num>` | an expression of that type | as above, checked first |
| `$x: ident` | a name | the name itself, resolved **at the call site** |
| `$x: block` | a `{ … }` statement block | the statements, wherever `$x` appears |

A macro whose result is a type may be used anywhere an expression is; a macro
whose result is `stmts` is a statement.

A macro that takes a `block` parameter is called with the block after the call,
the way a body-taking block is:

```rav
count_up(3) { looks::say("hi"); }
```

The `$` is only written where a parameter is *declared* and where its value is
*used*. In between, a `block` parameter stands on its own line as `$body;`.

## `fn` is a macro

An `fn` is `macro` with expression parameters and an expression result:

```rav
fn hypot(a: num, b: num) -> num { operators::mathop(MathOp::Sqrt, a * a + b * b) }

// is precisely

macro hypot($a: expr<num>, $b: expr<num>) -> num {
    operators::mathop(MathOp::Sqrt, $a * $a + $b * $b)
}
```

`fn` parameters are written the way parameters usually are — `a: num`, used in the
body as `a` — and the compiler rewrites those uses into parameters before the
expander runs. That rewrite is the whole of the sugar.

`fn` exists because that shape is common enough to deserve short syntax. It is
not a second mechanism: an `fn` is inlined at every call site, so

```rav
let h = hypot(3, 4);
```

emits the body's blocks where the call was, and nothing else — no custom block,
no call, no return cell. The `let` around it is the only thing that costs
anything, and that is one cell write.

An `fn` may not contain statements. That is not a stylistic rule; it is the
consequence of being a substitution, and it is why an `fn` cannot loop, branch or
recurse. If you need any of those — or a value that has to come back from a real
Scratch custom block — write a `proc` with a result type, and read
[what the call costs](/raven/design#value-returning-procedures).

## Hygiene

Two different rules, for two different kinds of name.

**Names the macro introduces** are the macro's own. Every expansion renames them:

```rav
macro count_up($times: expr<num>, $body: block) -> stmts {
    let counter = 0;             // a fresh cell per expansion
    repeat $times { $body; counter += 1; }
}

on flag_clicked {
    let counter = 99;            // the caller's cell, untouched
    count_up(3) { looks::say("hi"); }
    looks::say(counter);         // still 99: the macro used its own cell
}
```

The generated cell is real — it is the script's `_stackN` cell, or a `_vms` cell
when the expansion is inside a `proc`, and it appears in `raven expand` and in
`raven build --debug` — but it is addressed by index, so it cannot collide with the
caller's names and cannot appear in the editor's variable list. There is no
variable list to appear in. The cells are handed out in source
order, so they are stable across builds.

A macro cannot make a name another script can read, because raven has no such
names. If two scripts need to share a value, it belongs in a `var` — declared at
target scope so both can see it — and neither the macro nor the caller has to
name a Scratch variable to get one.

**Names passed in** resolve at the call site: a macro that takes `$x: ident` and
writes `$x = 0;` assigns the cell the caller's `x` names, in the caller's target.
A macro may also declare a `var` under a name the caller supplied, which is how
`for` gives you the counter you asked for. The declaration is written with the
parameter:

```rav
pub macro for_range($i: ident, $from: expr<num>, $to: expr<num>, $body: block) -> stmts {
    let $i: num = $from;              // the caller's name, a fresh cell
    repeat_until $i >= $to {
        $body;
        $i += 1;
    }
}
```

Because `$i` is an `ident` parameter it is not renamed, so the binding is the
name the caller wrote, in the caller's block — which is why `i` is readable after
the loop.

## Substitution and the once rule

A parameter is textually substituted — copy, not call. That is what makes `fn`
free, and it is also the only way a macro could surprise you:

```rav
pub macro twice($x: expr<num>) -> stmts {
    motion::move_steps($x);          // written into two statements
    motion::move_steps($x);
}

twice(sensing::timer())              // error: `$x` is evaluated twice, and
                                     // `sensing_timer` is sampled
twice(2)                             // fine: a literal is pure
```

The compiler applies the [purity rule](/raven/types#purity-pure-sampled-effectful)
to every substituted expression, counting the *statements* it lands in. Uses
inside one statement are always safe — nothing runs between them — which is why an
`fn` may use a parameter as often as it likes, and why a macro that expands to a
single expression is unrestricted.

## Totality

Macro expansion must terminate, and the way raven guarantees it is by refusing
recursion:

* a macro body may call other macros;
* the call graph must be acyclic;
* a cycle is an error, reported at the call that closes it;
* expansion stops after 32 levels, and a program that deep is an error too — the
  bound is what keeps a macro that expands into a slightly larger call of itself
  from running the compiler out of stack before the cycle check can name it.

There is no conditional expansion and no compile-time evaluation, so the set of
macros a program uses is decidable by looking at it. A `#[cfg]`-style feature, a
`const fn`, or a comptime loop would each break this, and each is therefore out of
the language.

## The prelude

`std::prelude` is imported into every file. It is a normal raven module —
`crates/raven/src/prelude.rav` — so you can read it, and every item in it is an
ordinary macro you can shadow.

| Macro | Expands to | Cost |
| --- | --- | --- |
| `while c { … }` | `repeat_until !c { … }` — the `!` is an `operator_not` | 2 extra blocks |
| `for i in a..b { … }` | a `_stackN` cell for `i`, then `repeat_until` | 9 blocks and one cell |

`+=` and the other compound assignments are core forms, not macros, because the
lowering depends on where the target lives; `match`, `f"…"`, compound assignment
and `return` are keywords, and are on the
[cost table](/raven/lowering#the-cost-table) with everything else.

A program may redefine a prelude macro: the program's definition wins, and the
expansion is printed either way.

## A macro cannot nest inside itself

Expansion is one pass with a call stack, and the stack is not unwound until the
whole expansion is lowered — so a `for` written inside another `for`'s body is
reported as a cycle, even though it is not one:

```
error: `for_range` expands into itself
  = note: the cycle is for_range → for_range
```

Write the outer loop as a core form and the problem disappears:

```rav
let L: num = 0;
repeat 4 {
    for i in 0..4 { board[L * 4 + i] = 0; }
    L += 1;
}
```

`repeat`, `forever`, `if` and `match` are keywords rather than macros, so they
may nest freely. This is a wart in the expander, not a design decision.

## Errors in expanded code

Errors inside generated code report the line *you* wrote, and add the definition
of the macro as a note:

```text
error: `count_up` expects `num` for `$times`, found `str`
  --> src/sprites/player.rav:14:9
   |
14 |         count_up(name, { looks::say("hi"); });
   |                  ^^^^
   = note: it is declared as count_up($times: expr<num>, $body: block) -> stmts
   = note: it is defined in src/lib/loops.rav
```

The expansion is a note, not the primary span, and it never points at generated
text. That is [law 11](/raven/design#_11-a-diagnostic-points-at-the-line-you-wrote),
and it is the difference between a macro system you can debug and one you cannot.

## Writing your own

Because macros are ordinary items, a project can grow its own sugar. A shared
module of macros is a shared module with no runtime cost and nothing to copy —
unlike a `proc`, which [modules](/raven/modules) explains must be duplicated per
target.

```rav
// src/lib/shapes.rav
pub macro square($size: expr<num>) -> stmts {
    repeat 4 {
        motion::move_steps($size);
        motion::turn_right(90);
    }
}
```

```rav
// src/sprites/player.rav
use lib::shapes::square;

sprite "Player" {
    on flag_clicked { square(50); }
}
```
