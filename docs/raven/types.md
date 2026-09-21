# Types and shapes

Scratch has one runtime value type — everything is a string that is usually
numeric — and two block shapes: round (reporters) and hexagonal (booleans).
raven's type system is a *discipline* laid over that, not a different target.

```text
type   = num | str | bool | list<T> | map<K, V> | Ident
scalar = num | str | bool
```

A type decides four things: which operators accept a value, which block inputs it
fits, how much storage it takes, and where it may be used at all.

## The types

| Type | Shape | What it is | Where it lives |
| --- | --- | --- | --- |
| `num` | round | a number | one cell |
| `str` | round | text | one cell |
| `bool` | **hexagonal** | a yes/no answer | one cell, one list item, one map value or one field |
| `list<T>` | round | a Scratch list | a Scratch list |
| `map<K, V>` | round | a key/value table | one Scratch list of alternating pairs |
| `Point`, `Segment`, … | *not a value* | a declared `struct` | a run of cells |

Where a cell lives depends on how long it has to live. A `let` in a script is on
that script's **stack**, pushed when its declaration runs and popped when its
block ends. A `var` — and a `proc`'s frame, which recursion shares — lives in the
target's arena, which is grown on demand and never shrinks. See
[the memory system](/raven/design#the-stack-grows-and-shrinks).

## Booleans: stored as a value, converted on the way out

A boolean can be kept in a cell, a list item, a map value or a struct field, and
a `proc` can return one. Two things make that work.

Scratch has no `true`/`false` **block**, so a boolean *literal* is lowered to the
one comparison that has that constant value: `true` is `<1 = 1>` and `false` is
`<1 = 0>`.

And a boolean *input* is hexagonal — it is wired to a block, never to a value —
so a boolean read out of storage is wrapped in a comparison:

| written | emitted | why |
| --- | --- | --- |
| `var live: bool = false;` | the arena's item is the boolean `false` | a declaration is loaded, not run |
| `live = 1 > 0;` | `replace item 1 of _vms with (1 > 0)` | what is stored is the block's value |
| `if live { }` | `<item 1 of _vms = "true">` | a stored value becomes a block |
| `if true { }` | `<1 = 1>` | a literal is a constant comparison |
| `f"{live}"` | the same comparison, in a text slot | Scratch prints a boolean as `true`/`false` |

That comparison is right for **both** shapes a stored boolean can take. Scratch
keeps a comparison's result as its own `true`/`false`; a list loaded from
`project.json` may hand back the *text* `"true"`/`"false"`. `Cast.compare` falls
back to `String(value)` as soon as one side is not a number, and `String(true)`
is `"true"` — so `<stored = "true">` reads either one correctly.

```rav
var live: bool = false;        // an arena cell
var flags: list<bool> = [];    // a list
var seen: map<str, bool> = []; // a map

live = score > 0;
flags.push(live);
seen.set("live", live);

if live { }                    // all three read back as conditions
if flags.at(1) { }
if seen.get("live") { }
```

`num` and `str` are distinct even though Scratch blurs them, because the blur is
where bugs live.

## Values, containers and places

The three rows of that table behave differently, and the difference is the point:

* **`num`, `str`, `bool` are values.** They are copied. A binding holds one of
  them in one cell.
* **`list<T>` and `map<K, V>` are containers.** They are Scratch lists, reached
  through raven's checked methods — `xs.push(v)`, `m.get(k)` — and never through
  a raw name.
* **A `struct` is a place.** It is not a value at all: it cannot be assigned,
  compared, returned, or passed to a `proc`. `p.x` reads one cell at a constant
  offset, `p.x = e` writes one cell, and `seg.to.x` adds two compile-time offsets
  into one constant.

That last rule is what removes a whole class of questions. A struct has no
identity beyond its cells, so there is no copy to make, no aliasing to worry
about, and no lifetime to track.

```rav
var name: str = "player";
var score: num = 0;

score = score + 1;            // fine
score = score + name;         // error: `name` is str, `+` needs num
score = score + num(name);    // explicit, and free
looks::say(f"{name}: {score}"); // fine
```

## Shapes: where a value may go

The block catalog records each input's `Shape` — `Number`, `Whole`, `Text`,
`Bool`, `Color`, `Variable`, `List`, `Broadcast`, or a menu. raven turns that
table into the rule set:

* `Number`, `Whole`, `Integer`, `Angle`, `Positive` accept `num`.
* `Text` accepts `str` and `num`.
* `Bool` accepts only `bool`.
* `Color` accepts a `"#rrggbb"` string literal.
* `Broadcast` accepts the name of a declared broadcast.
* `Menu(…)` accepts the matching menu type (see [standard library](/raven/std)).
* `Variable` and `List` are not value slots at all — they are covered by core
  syntax, or by passing the list value itself.

Because the catalog is the source of these rules, a new block arrives with its
types already enforced.

## Conversions

Two conversions exist. Both are **free**: the target is untyped, so neither emits
a block.

```rav
let a: num = num("12");        // no block: the value is retyped
let b: str = str(score);       // no block: the value is retyped
```

They change what the checker lets you do with a value, and nothing else. In
particular they do not round, trim or parse — Scratch's own coercion does that,
at the moment the value reaches a block.

There is **no `bool(x)`**. Converting a number or a string to a boolean has no
single meaning, and the block would have to guess. Write the comparison you
meant:

```rav
if score != 0 { }              // not: if bool(score) { }
if answer == "yes" { }         // not: if bool(answer) { }
```

## Truth and comparison

raven inherits Scratch's comparison semantics, and does not hide them:

* `==` compares numerically when both sides look like numbers, and
  **case-insensitively** as text otherwise. `"Apple" == "apple"` is `true`.
* `<`, `>`, `<=`, `>=` compare numerically; non-numeric text compares as `0`.
* `&&` and `||` evaluate **both** operands. There is no short-circuiting; see
  [design laws](/raven/design#short-circuit-and).

## Variables

A variable is declared once, at target scope, with a type and an initial value:

```rav
var score: num = 0;              // one cell of the target's `_vms`
var trail: list<num> = [];       // a Scratch list
var totals: map<str, num> = [];  // a Scratch list of key/value pairs
pub var best: num = 0;           // one cell of the project's `_gvm`
```

* Without `pub`, the variable belongs to the file's target. In a stage file that
  makes it project-wide, because the stage's variables always are.
* With `pub`, a **scalar** goes into `_gvm`, the arena the stage declares, so it
  has exactly one cell for the whole project. A `pub` list or map is a Scratch
  list on the stage.
* There is **no shadowing between `var`s**: a sprite-local variable may not share
  a name with a project-wide one. raven rejects it rather than quietly choosing.
* A `var` is never block-local. It is visible to every script in its target,
  exactly as in Scratch. For a value that belongs to one block, use `let`.
* A `struct` belongs to the target that declares it. `pub struct` parses, but it
  buys nothing: a struct is a frame in one target's arena and cannot be imported,
  so it is only usable where it is declared.
* A struct occupies at most **64 cells**, fields and nested fields together. The
  bound is what keeps a typo from declaring a frame that makes `project.json`
  enormous.

No `var` is a Scratch variable a program can name: there is no
`data_variable("score")` to write, no name a running program can look up, and no
built-in monitor. The one Scratch variable a project can have is the mirror
`watch` declares, which exists so a cell can be read on the stage — see
[the design laws](/raven/design#_7-there-is-no-raw-variable-access-only-the-virtual-memory-system).

An initializer is a literal or a list of literals, or a struct literal. raven
does not compute them, because the editor does not: if you want a computed
starting value, assign it in an `on flag_clicked` script. Because the starting
value is part of the arena's declaration, a `var score: num = 0;` really does
hold `0` before any script runs.

## Monitors

A watched value gets a monitor, and where it goes is the editor's decision
unless the code makes it: a cell that is watched declares a real Scratch
variable with `visible` set, and the editor places a monitor it is not given a
position for exactly as it places one for a variable made by hand. Nothing about
the layout is a compiler's guess, and `at X Y` (on a `var` or `list` in
raven-asm) pins one down when a project wants an exact spot.

## Lists

A list is the one container Scratch gives blocks to drive an item at a time, so a
`list<T>` is a Scratch list — reached through raven's checked methods rather than
by naming it at run time.

```rav
var trail: list<num> = [];

trail.push(5);
trail.insert(1, 9);
trail[1] = 100;                        // data::replace_item_of_list
let lead = trail[1];                   // data::item_of_list
let same = trail.at(1);                // the same block
let count = trail.len();
let where = trail.index_of(5);         // 1-based, or 0 when it is not there
let first = trail.first();             // item 1
let last = trail.last();               // item (length of trail)
let whole = trail.text();              // the whole list, one block
trail.remove(1);
trail.pop();
if trail.contains(5) { }
if trail.is_empty() { }                // length == 0
trail.clear();
```

`trail[i]` and `trail[i] = v` are sugar for the same two blocks; both spellings
emit exactly one block. The `data::` functions work too — `data::add_to_list(5, trail)`
is `trail.push(5)` — and both resolve the list against the target's declarations,
so neither can name a list the program did not declare.

List indexes are `1`-based, as in Scratch, and are not checked at compile time —
raven cannot know a list's length. An out-of-range read is `""`, and an
out-of-range write is ignored, both of which the runtime documents.

## Maps

A `map<K, V>` is one Scratch list holding the pairs `k0, v0, k1, v1, …`, so it
needs no machinery beyond the list blocks:

```rav
var totals: map<str, num> = [];

totals.set("runs", 3);      // an `item # of` lookup, then a replace or an append
let runs = totals.get("runs");
let known = totals.has("runs");
totals.remove("runs");
let n = totals.len();       // the list's length over two
if totals.is_empty() { }
totals.clear();
```

Keys and values are scalars. `get` on a missing key is `""`: a missing key has
position 0, and `item 1 of` a list is its first *key*, so the read is guarded
rather than trusting the lookup.

## Structs

A `struct` is a named shape: a fixed run of cells, one per scalar field, in
declaration order.

```rav
struct Point { x: num, y: num }
struct Segment { from: Point, to: Point }

sprite "Walker" {
    var home: Point = Point { x: 0, y: 0 };

    on flag_clicked {
        home.x = home.x + 1;                     // one cell write
        let line: Segment = Segment {
            from: Point { x: 0, y: 0 },
            to: Point { x: 3, y: 4 },
        };
        looks::say(line.to.x);                   // two offsets, one block
    }
}
```

* A struct is a **place**, not a value, so it cannot be assigned, compared,
  returned, or passed as a `proc` argument, and it cannot live in a list or a map.
* A field's offset is decided while compiling, so nesting is free:
  `line.to.x` is one `data_itemoflist` at a constant index.
* A struct field may be another struct; it may not be a list or a map.
* A struct literal is only written where a place is being made — the initializer
  of a `let` or a `var` — because `if ready { … }` ends a condition with a name
  and `ready {` must not become a literal.
* A declaration's field values are literals, like every other declaration; a
  `let`'s may be expressions.

## Procedures and parameters

Every `proc` parameter states its type, and that type decides the reporter the
body must use:

| Declared | Scratch parameter kind | Read with |
| --- | --- | --- |
| `x: str` | `%s` | `argument_reporter_string_number` |
| `x: num` | `%n` | `argument_reporter_string_number` |
| `x: bool` | `%b` | `argument_reporter_boolean` |

Inside a `proc`, the parameter name is in scope as a value of its declared type.
raven emits the reporter for you; there is no way to call a string parameter
through a boolean reporter, which was a class of bug raven-asm prevented by
spelling and raven prevents by type.

`warp` marks the custom block *run without screen refresh*, and is part of the
definition's mutation, exactly as in raven-asm.

A `proc` may also declare a result type — `proc f(a: num) -> num { … }`. The
declared type may be any type the checker knows; the value travels through a cell
of `_vms` like any other value, and a `bool` one is read back as `<cell = "true">`,
so a call can be a condition. `return e;` is checked against the declared type, and
a call used as a value has that type. See
[design laws](/raven/design#value-returning-procedures) for what the call costs.

## Locals

`let` declares a name whose type comes from its initializer, or from an
annotation you may write:

```rav
let total = 0;                 // num, inferred
let best: num = 0;             // the same, written down
let label = f"score: {total}"; // str
```

A `let` is a **cell**, so it is block-scoped, mutable and shadowing, and it costs
one `data_itemoflist` per read and one write per assignment. Which list holds it
depends on where it is written: outside a `proc` it is pushed on the script's own
`_stackN` and popped when the block ends, while inside a `proc` it is a cell of
the target's `_vms` — a script's stack belongs to the script, so a procedure's
locals live in the arena. It adds nothing to the project's variable list:

```rav
on flag_clicked {
    let speed = 10;
    repeat 3 {
        let speed = speed + 5;   // a new cell; the outer one is untouched
        motion::move_steps(speed);
    }
    motion::move_steps(speed);   // still 10
}
```

Because a cell read is a plain reporter, `sampled` and `effectful` expressions are
perfectly safe in a `let`: the expression runs once, when the cell is written.

A cell is a location, not a stack frame, so a `let` inside a procedure is shared
by every call of it. raven refuses a `let` inside a *recursive* procedure and
tells you to pass the value as a parameter; the same reasoning applies to a
procedure that can be entered twice at once, which is why a procedure that waits
should take what it needs as a parameter, or be written `warp`.

## Purity: `pure`, `sampled`, `effectful`

Every block in the catalog carries a purity class:

| Class | Meaning | Examples |
| --- | --- | --- |
| `pure` | same inputs, same result, always | `operator_add`, `operator_join`, `operator_length` |
| `sampled` | reads the world; two evaluations may differ | `motion_xposition`, `sensing_timer`, `sensing_answer`, `operator_random` |
| `effectful` | a command; it changes something | `motion_movesteps`, `data_addtolist` |

A cell read is `pure`: reading the same cell twice cannot give two answers,
because a cell is a name for a location rather than a sensor. That is why a
variable used as a `match` subject needs no copy, while `sensing::timer()` does.
`m.get(k)` is the one method that is not pure — it fills two cells of its own on
the way — so it is never duplicated.

The class exists for one rule, and the rule exists because a macro parameter is a
substitution:

> **A macro parameter written into more than one statement of the expansion must be
> given an expression that may be evaluated more than once.**

```rav
macro twice($x: expr<num>) -> stmts {
    motion::move_steps($x);          // written into two statements
    motion::move_steps($x);
}

twice(sensing::timer())              // error: `$x` is evaluated twice
twice(2)                             // fine: a literal is pure
```

Uses *within* one statement are always safe, because nothing runs between them —
which is why an `fn` may use a parameter as often as it likes.

`let` is not affected by this rule, and that is exactly what it is for: `let x =
motion::x_position();` reads the sensor once and every later use of `x` reads the
cell. Bind a value you want to sample once, and pass a pure expression to a macro.

## A list is as long as it is used

Scratch's `replace item` does nothing at all when the list is shorter than the
index, which would make `xs[i] = v` a silent no-op on a fresh list. raven grows
the list to the index first, so a list is exactly as long as the highest index
anything has written to it:

```rav
var xs: list<num> = [];
xs[3] = 7;      // three items: "", "", 7
xs[1] += 5;     // the first of them is now 5
```

That is one `if` and one `repeat until` around the write. Reading past the end
is still the empty string, as it is in Scratch; only writing grows the list.
