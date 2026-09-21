# Design laws

raven is defined as much by what it refuses as by what it offers. These are the
rules the compiler is not allowed to break, each with the failure it exists to
prevent.

## 1. One block per statement, all the way down

raven-asm's rule is inherited, not weakened: a raven statement eventually becomes
Scratch blocks that *are* those statements, and a raven expression becomes
reporter blocks that *are* that expression. raven adds no runtime of its own — no
dispatch, no frame layout, no bookkeeping. The lists it declares, `_vms` and
`_gvm`, are fixed arrays indexed by constants, and every access to them is a
block the source asked for. If a construct cannot be expressed as blocks, it is a
compile error, not a clever trick.

## 2. Every expansion is printable

`raven expand` prints the raven-asm for any item, and `raven build --debug`
writes the whole program as `.rasm` files. A construct whose cost you cannot see
is a bug in the language, not a feature of the compiler.

This is the rule Scrust was missing. It had desugaring, but the desugaring lived
in the compiler and produced only blocks; there was no way to look at what a `for`
loop had become.

## 3. Sugar is a macro, or it is a keyword with a written lowering

Every convenience whose shape is fixed — `while`, `for`, `loop` — is a macro in
`std::prelude`, and the compiler knows no special case for any of them. They are
still *keywords*, because the parser has to recognise the form to rewrite it into
a call to `while_loop`, `for_range`, `for_range_inclusive`, `for_each` or
`loop_forever`; the prelude then defines those macros, so replacing it with a
different definition of `while_loop` changes what `while` means without touching
the compiler. The consequence is that the language can grow without the compiler
growing, and a user can add sugar the standard library does not have.

Some constructs are keywords instead, because their shape is not a substitution:

| Form | Why it is not a macro | Lowering |
| --- | --- | --- |
| `f"…"` | a variable number of holes | one `operator_join` per piece after the first |
| `match e { … }` | a variable number of arms | one `control_if_else` per arm |
| `x += e` and friends | the *target* decides the lowering: a scalar cell is a read, an operator and a write; a list element is a `data_itemoflist` and a `data_replaceitemoflist` | [cost table](/raven/lowering) |
| `p.x` and `p.x = e` | the offset is a compile-time constant | `data_itemoflist` / `data_replaceitemoflist` on the cell |
| `xs.push(e)`, `m.get(k)` | the receiver's type decides which block it is | one or two blocks, or a guarded read |

None is a hiding place: every lowering is one or two lines, every one is on the
[cost table](/raven/lowering), and every one is visible in `raven expand`.

## 4. Expansion is total

A macro expands to raven-asm by a finite, acyclic substitution. There is no macro
recursion, no expansion loop, no compile-time evaluation, no conditional
compilation. The macro call graph must be a DAG, and a cycle is an error naming
the cycle.

This is a real restriction, deliberately taken. It means:

* compilation always terminates, for every input;
* the cost of a program is decidable by reading it;
* no `.sb3` was ever produced by a program that ran at compile time.

Anything expressible only with a compile-time loop belongs in a generator that
writes `.rav`, not in the language.

## 5. Types are declared, and conversions are explicit

`num`, `str`, `bool`, `list<T>`. A declaration says its type; a call site must
match it. There is no implicit conversion in either direction and no inference
from use.

The two conversions that exist are free, because the target is untyped anyway:
`num(x)` and `str(x)` change how the checker treats a value without emitting a
block. Everything else — including `bool` — requires you to write the comparison
you meant.

## 6. A statement is a statement; an expression is an expression

Scratch has reporter blocks and it has stack blocks, and there is no construct
that is both. raven mirrors that exactly: `if`, `repeat`, `forever` and their
relatives are statements, and they may not appear inside an expression.

This is why raven has no ternary and no conditional expression. Every "expression
`if`" that has ever been proposed for a Scratch language is either a lie (it
compiles to two eager branches and arithmetic) or a hidden script split. raven
declines both and tells you to use an `if` statement.

## 7. There is no raw variable access, only the virtual memory system

Scratch's data blocks take a variable's **name** at run time. A raven program
never writes one. A block that names a Scratch variable — `data_variable`,
`data_setvariableto`, `data_changevariableby`, `data_showvariable`,
`data_hidevariable` — is reached by raven and refused by it, with the reason:

```text
error: `data::set_variable_to` is not available in raven
  = note: variables are VMS cells; write `n = value;`
  = note: raw Scratch variables and lists are the layer raven compiles *to*,
          not the layer a program writes
```

A raven program cannot *write* a Scratch variable: everything it stores is a
**cell of the virtual memory system**: a constant index into one of two Scratch
lists, read with `data_itemoflist` and written with `data_replaceitemoflist`. The
one Scratch variable a project can declare is the mirror `watch` asks for, and
that exists to be looked at on the stage rather than programmed with.

There are four lists, and the split between them is by **lifetime**:

| | |
| --- | --- |
| `_vms` | the target's arena: its `var`s and every `proc`'s frame. Code-sized, and it outlives a script. |
| `_gvm` | the project's arena, declared on the stage: every `pub var`, and every stage `var` |
| `_stack1`, `_stack2`, … | one per script: everything block-scoped. **It grows and shrinks with the scopes that use it.** |
| `_console` | the log, when something logs |

None of them is declared unless the program uses it: a target with no `var`, no
`let` and no `proc` cell emits no arena at all, a script that never pushes a
block-scoped cell gets no stack, and a project that never logs gets no console.
The memory manager's `warp` procedure follows the same rule — it exists exactly
when there is an arena to grow.

### The stack grows and shrinks

A `let`, a `for` counter, a struct `let` or a temporary is a cell on the
**running script's** stack. `let x = e;` is one `add e to _stack1`; the read is
`item 2 of _stack1`; and when the block that declared it ends, the block pops it
again with `delete 2 of _stack1`. Nothing is reserved up front: the list starts
**empty** and its length is exactly the number of block-scoped cells alive at
that moment. A script also empties its own stack at the start of every run, so a
script stopped mid-block cannot leave the next run out of step.

Each script has its own stack, so two scripts running at once — the green flag
and a broadcast, say — cannot pull the ground out from under each other.

### The arenas are grown on demand

`_vms` and `_gvm` hold what has to outlive a script: a `var`, and a `proc`'s
frame, which every call of that procedure shares (that is what makes recursion
work with a constant index). They are declared **empty** too, and a generated
`warp` procedure — the whole of the memory manager — grows them to the highest
cell the program can reach and writes the declared starting values, once:

```rasm
proc __vms_reserve() warp {
    control_if(operator_lt(data_lengthoflist("_vms"), 12)) {
        control_repeat_until(operator_not(operator_lt(data_lengthoflist("_vms"), 12))) {
            data_addtolist("", "_vms");
        }
        data_replaceitemoflist(1, "_vms", 0);
    }
}
```

Every script calls it first; after the first call it is one block. Its size is
the program's, not the data's — no amount of running makes it bigger — so there
is no storage limit a program can hit, and nothing is reserved that the program
does not use.

Both arena names and the stack prefix are reserved: a program cannot declare
them.

A cell in an arena is a **name for a location**, not a stack frame, and that has
one consequence worth stating plainly: a `let` inside a *procedure* is one cell
shared by every call of that procedure. raven refuses a `let` in a *recursive*
procedure for exactly that reason, and tells you to pass the value as a
parameter instead. A `let` in a **script** has no such caveat: it is on that
script's own stack, pushed when the declaration runs and popped when the block
ends.
Two scripts of the same target that can enter the same procedure at the same time
share it too — so a procedure that waits, and is called from more than one place,
should take what it needs as a parameter, or be written `warp` so it cannot yield
halfway.

The payoff is that the storage of a raven program is *finite and known*. There is
no way to make a variable the editor can see, no way to reach one by name, and no
way for a macro to quietly add one. `raven expand` prints every cell, and the
count of them is the count in the source.

### Values, places and containers

Three kinds of storage follow from that, and the difference between them is the
difference between Rust's values, its owned places and its reference types:

| | what it is | how it is read |
| --- | --- | --- |
| `num` `str` `bool` | a value: it is copied | one cell read |
| `struct` | a **place**: a run of cells whose shape is fixed at compile time, so it cannot be copied, compared or passed to a `proc` | one cell read per field, at a constant index |
| `list<T>` `map<K,V>` | a container: a Scratch list, because that is the one thing Scratch gives blocks to drive an item at a time | the block the method is, through raven's checked name |

A struct being a place is the whole reason `p.pos.x` costs one block: the layout
is decided while compiling, so the two field offsets are added into one constant.
Nesting costs nothing. A struct has no identity beyond its cells, so there is no
ownership question to answer and no hidden copy to get wrong.

### A boolean is stored, and converted on the way out

Scratch's boolean inputs are hexagonal, and every one of them is wired to a
*block*. A cell, a list item and a struct field hold *values*. raven bridges the
two in one place, and only one: **a boolean read out of storage is wrapped in a
comparison**.

```rasm
// live = score > 0;          -- what is kept is the block's value
data_replaceitemoflist(1, "_vms", operator_gt(score, 0));

// if live { … }             -- what is read is turned back into a block
control_if(operator_equals(data_itemoflist(1, "_vms"), "true")) { … }
```

`<stored = "true">` is right for both shapes a stored boolean can take. Scratch
keeps a comparison's result as its own `true`/`false`; a list loaded from
`project.json` may hand back the *text* `"true"`/`"false"`. `Cast.compare` falls
back to `String(value)` the moment one side is not a number, and `String(true)` is
`"true"`.

A boolean *literal* has no block in Scratch either, so `true` is lowered to `<1 =
1>` and `false` to `<1 = 0>` — a constant comparison, one block, correct
everywhere a boolean can go. That is what lets a `var`, a `let`, a `list<bool>`,
a `map<K, bool>`, a struct field and a `proc p() -> bool` all hold one.

### Maps

A `map<K, V>` is one Scratch list holding `k0, v0, k1, v1, …`. `has` is
`item # of` compared to zero; `set` is that lookup followed by a replace or an
append; `remove` deletes the key and then the value that followed it. `get` is
the one method that costs a `control_if`: a missing key has position 0, and
`item 1 of` a list is its first *key*, so the read is guarded rather than
trusting the lookup.

## Value-returning procedures

A `proc` may declare a result type and `return` a value:

```rav
proc clamp(value: num, low: num, high: num) -> num {
    if value < low { return low; }
    if value > high { return high; }
    return value;
}
```

`return` really returns: it is a `control_stop` on the custom block, so the
statement stops the procedure rather than falling through, which is where
Scrust's version went wrong. Because `control_stop` has no bottom notch, a
statement after a `return` is a compile error — raven says so, at the statement
that can never run.

The value travels through the procedure's **own cell in `_vms`**, and a call in
expression position is hoisted one statement and read back:

| | |
| --- | --- |
| `return e;` | `data_replaceitemoflist(R, "_vms", e)` then `control_stop("this script")` — and when `e` is a boolean, the call site reads it back as `<cell = "true">` |
| `f(x)` used as a value | the `procedures_call`, then one `data_replaceitemoflist` copying cell `R` into a temporary cell, and a `data_itemoflist` for the temporary |
| `f(x);` as a statement | the `procedures_call`, and nothing else |

Hoisting keeps evaluation left to right and keeps a call from crossing a
statement that could observe it, and the copy is why `f(1) + f(2)` reads two
different values rather than the same one twice. All of it is printed by
`raven expand`.

## 8. Ownership is declared, never inferred

`pub` on a declaration is what puts it on the stage and makes it visible to every
sprite. Without it, a declaration belongs to the file that declares it. A module
has no target of its own, so anything a module exports is project-wide by
construction — a module cannot bind one name to two different things for two
different users.

`use` is inclusion, not Rust's name import: a Scratch custom block belongs to one
target, so importing a `proc` copies it into each importer, and raven says so in
the documentation and in `raven expand`. Macros and `fn`s are compile-time, so
importing them copies nothing at all.

## 9. Nothing is duplicated silently

A macro parameter is substituted where it is used, so a parameter a macro writes
into more than one statement is evaluated more than once — fine for a pure
expression, wrong for one that samples the world.

raven classifies every block in the catalog as `pure`, `sampled` or `effectful`,
and enforces the rule: a parameter mentioned in more than one statement of an
expansion must be given something the compiler may copy. Uses *within* one
statement are always safe, because nothing runs between them — which is why an
`fn` may use a parameter as often as it likes.

A `let` is not affected by this rule any more, and that is the point of it being
a cell: `let x = motion::x_position();` reads the sensor once, and every later
use of `x` reads the *cell*. Nothing is re-evaluated, so nothing is duplicated.

## 10. Every block is reachable from raven

The block catalog is the standard library. Every catalog block has a raven
binding — a core form, a std function, a menu type, a hat, or an explicit refusal
— and a test in the `raven` crate fails the build if a catalog block has no row.
Adding a block to Scratch support cannot leave raven quietly behind.

A binding is one of three things: **callable** (`motion::move_steps(10)`),
**syntax** (a keyword or declaration that covers it), or **forbidden**, with the
reason. The five blocks that name a Scratch variable are forbidden, because raven
does not declare Scratch variables; see [law 7](#_7-there-is-no-raw-variable-access-only-the-virtual-memory-system).

Where a callable block names a project entity rather than a value — a costume, a
sound, a broadcast, a list — its parameter takes the **name**, and the checker
resolves it against the declarations of that kind. The one thing a block may
never take is an expression where it wants a mention, or the reverse, and the
checker enforces the distinction from the catalog's own `Wire` field.

That name is written as the literal the project declared it as —
`looks::switch_costume_to("idle")` — rather than as an invented variant, because
the name is the author's and any second spelling of it is both confusing and
lossy. A dropdown whose values Scratch defines is the other case and is an enum,
`Goto::MousePointer`. See [the standard library](/raven/std#menu-types).

## 11. A diagnostic points at the line you wrote

Every diagnostic carries the source line and a caret, plus `= note:` lines that
explain the fix. Code produced by a macro is never blamed: an error inside an
expansion reports the call site first and adds a note naming the macro and where
it is defined.

The compiler never panics on malformed input, and never reports "parsing stopped
early" in place of an error.

## 12. The project is the source tree

There is no cached intermediate that changes what a program means, no lock file
that selects a different meaning, and no state in the build directory that a clean
checkout lacks. Identifiers are derived from content and the archive timestamp is
pinned, so the same source produces the same bytes.

---

## Rejected designs

Each of these has been considered and refused. They are listed because the
reasoning matters more than the list.

### Conditional expressions

`if c { a } else { b }` in expression position cannot be lowered to a Scratch
reporter. Vanilla Scratch has no select or ternary block, and a boolean reporter
cannot legally be plugged into a numeric input. The three possible lowerings are
all unacceptable:

* **arithmetic selection** (`a·c + b·(1−c)`) evaluates both branches, is wrong for
  strings, and silently depends on boolean-to-number coercion the editor will not
  let you build by hand;
* **both branches in a variable** requires a hidden variable, violating
  [law 2](#_2-every-expansion-is-printable) unless it is a macro — and a macro
  cannot fix that the branches are eager and the result is a different shape;
* **splitting the script at the call site** is Scrust's approach, and it breaks
  inside any enclosing expression.

Use an `if` statement. It costs one block.

### Value-returning functions that are not procedures

A `proc` that returns a value is supported, with the exact cost printed above.
What is still refused is a *function you can call from inside an expression as if
it were free*. A value that comes from a script needs a call and a read, and raven
makes you see both. If the computation is pure, `fn` inlines it and costs nothing;
if it needs state, the `proc`'s cost is on the page.

### Block-local storage that grows

`let` gives a block its own storage, but the storage is a fixed cell, not a
frame: there is no allocation, no stack pointer, and no per-call layout. What
raven refuses is the general allocator — memory whose shape depends on how the
program runs.

The two places that refusal is visible are both compile errors rather than silent
misbehaviour: a `let` in a recursive procedure, and a value that has to survive
across a procedure that yields. A recursive procedure that wants per-call state
passes a parameter, exactly as it would in Scratch.

A `list<T>` *does* grow, because Scratch's list blocks grow one — but it is a
Scratch list by construction, not a slice of the arena, and raven never computes
an index into it at run time.

### Structs by value, ownership and moves

Rust's answer to "who owns this" is a move checker. raven's answer is to not have
the question: a struct is a **place**, so it cannot be assigned, passed, returned
or compared as a whole, and the only way to give one field's value to someone else
is to read the field. That removes copies, aliasing and lifetimes in one rule, and
it keeps every field access a single block at a constant index.

What that refuses is a struct you can put in a `list`, hand to a `proc`, or
return from one. Each of those needs a memory layout that depends on the
program's run, which is what the virtual memory system deliberately does not have.

### A general `for` over anything, and iterators

An iterator is a hidden variable plus a protocol. raven's `for` counts a range
with a visible counter cell, or walks a named list with a counter and an element
cell, and both are in the expansion where you can see them. A `map` is not walked
this way; its keys are a list you can read. Nothing hides a state machine.

### `break` and `continue`

Scratch can leave a loop only by leaving the script (`control_stop` with *this
script*) or by setting the loop's condition. There is no `continue` at all. raven
therefore has no `break` or `continue` keyword; use
`control::stop(StopOption::ThisScript)` when that is what you mean.

### Short-circuit `&&` and `||`

Scratch's `operator_and` and `operator_or` evaluate both operands, always.
Writing `&&` and `||` in raven keeps the familiar spelling, but the
[types](/raven/types) page says plainly that both sides run, and that a guard
like `i != 0 && 10 / i > 1` will divide by zero.

Making it short-circuit would need statement lowering in expression position,
which [law 6](#_6-a-statement-is-a-statement-an-expression-is-an-expression)
forbids.

### Token-tree macros and compile-time evaluation

Rust's `macro_rules!` matches tokens and can recurse; that is a Turing-complete
sub-language with its own error surfaces. raven's macros are typed, hygienic,
acyclic substitutions. They are less powerful on purpose: the power is what made
the previous attempt unmaintainable.

### Attributes on items

`#[…]` attributes were considered for `warp` and event binding. They hide
meaning in a place that is easy to miss and impossible to type-check without a
second grammar. raven uses plain keywords (`proc … warp`, `on …`), which are
checked and visible.

### Implicit conversions and operator overloading

`"score: " + score` reads well until the day `score` is a string. raven has
neither. `f"score: {score}"` says concatenation, `num(x)` and `str(x)` say
conversion, and nothing says both.
