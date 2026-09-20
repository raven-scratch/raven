# Why no syntax sugar

raven-asm's defining rule is that **one statement is one Scratch block**. This page
explains what that rules out, why it is worth the verbosity, and how it relates
to the project that came before it.

## The problem with sugar

The obvious way to build a language that targets Scratch is to make it
*nicer than Scratch*. You add `for` loops, `+=`, string interpolation, `if`
expressions, function return values, and each of those compiles into the handful
of blocks Scratch needs.

It works, until it doesn't. Consider what happens as sugar accumulates:

* **The output stops matching the source.** `for i in 0..10 { say(i) }` becomes a
  variable declaration, a `set to 0`, a `repeat 10`, a `join`, a `say`, a
  `change by 1` — six blocks spread over two scripts, in an order nobody wrote.
* **Errors lose their home.** "This expression has the wrong type" is really
  "these four nested blocks disagree", and the source span you can point at is
  the whole construct.
* **Every feature multiplies.** A new sugar layer must know how to expand inside
  every other one. `return` inside a `for` inside a template function is a
  cross-product, and each combination is a fresh chance to be wrong.
* **You cannot round-trip.** Load a project somebody edited by hand in the
  editor, and there is no source that produces it.

[Scrust](https://github.com/DilemmaGX/Scrust) explored exactly this space and
reached that wall: multi-layer sugar made the compiler hard to extend and the
generated projects hard to reason about. raven-asm is the deliberate opposite
experiment.

## What raven-asm does instead

raven-asm keeps Scratch's vocabulary and adds only what a text file needs and a
block canvas does not:

| Need | Solution | Why it is not sugar |
| --- | --- | --- |
| Names, files, diffs | one file per target | A file *is* a target; no rewriting happens. |
| Many targets | `raven-asm.toml` | A list of files. |
| Reuse | `use` copies a `proc` into a target | Scratch custom blocks are per-target; copying is the only honest translation. |
| Structure | `var`, `list`, `broadcast`, `costume`, `sound` | These fill the target's data sections. They emit no blocks. |
| Errors | diagnostics with sources | Compile-time only. |

And then nothing else. Every statement is a block, every argument is a literal
or another block, and every construct maps to exactly one thing in the editor.

## What that costs, and what you write instead

The cost is verbosity. These are all mistakes, and here is the honest
replacement:

| You might expect | raven-asm |
| --- | --- |
| `if x > 5 { }` | `control_if(operator_gt(data_variable("x"), 5)) { }` |
| `x = x + 1;` | `data_changevariableby("x", 1);` |
| `x += 1;` | `data_changevariableby("x", 1);` |
| `for i in 0..10 { }` | `control_repeat(10) { }`, or `control_for_each("i", 10) { }` |
| `while cond { }` | `control_repeat_until(operator_not(cond)) { }`, or `control_while(cond) { }` |
| `"score: " + score` | `operator_join("score: ", data_variable("score"))` |
| `-x` | `operator_subtract(0, data_variable("x"))` |
| `f(x) + 1` | `operator_add(f(x), 1)` |
| `return v` | write to a variable, or restructure |

That is more typing. In exchange:

* **Every line is a block.** You can read the source and know the project, and
  read the project and know the source.
* **Errors point at one thing.** "This input needs a boolean" is a statement
  about one block.
* **The compiler is small.** It is a lexer, a parser, a lookup table and a block
  writer — `crates/raven-scratch/src/catalog.rs` is the language, literally.
* **Adding a block is a data change.** New opcode? Add a row to the catalog and
  it works, appears in the reference, and is validated the same way as
  everything else. There is no expansion logic to update.
* **The output is round-trippable in spirit.** Load the `.sb3` in the editor,
  edit a dropdown, and the change maps back to a one-line source edit.

## Is that a *language*, or just Scratch in a text file?

It is Scratch in a text file, and that is the point. raven-asm's contribution is
tooling, not semantics:

* a manifest and a multi-file layout;
* `use` for sharing procedures;
* compile-time checking that Scratch's editor gives you only by refusing to drop
  a block — dropdown values, arity, boolean slots, undeclared names, unreachable
  code after a cap;
* deterministic identifiers, so builds are reproducible;
* error messages with line numbers.

If you want the sugar, TurboWarp's compiler and Scrust both offer it. raven-asm is
what you use when you want the project you open in Scratch to be exactly the
project you wrote.

## The one place the mapping is two-to-one

`proc` and its calls. Scratch stores a custom block as a `procedures_definition`
holding a `procedures_prototype` with a mutation, and each call site as a
`procedures_call` with a matching mutation. There is no single-block
representation of "define a custom block", so raven-asm cannot invent one without
either dropping a Scratch feature or pretending the representation is simpler
than it is. One `proc` declaration maps to the definition pair; one call maps to
one `procedures_call`.

Everything else is one-to-one.

## Scope: this compiler is the core, not the whole story

Everything on this page describes *this* compiler, and the rule it follows is a
decision about scope rather than a claim about what raven-asm could be used for:

> raven-asm source is a complete, literal description of a Scratch project.

The compiler reads it and writes it out. It never turns one construct into
several blocks, so the `.sb3` is legible in terms of the source and the source is
legible in terms of the `.sb3`. That property is what makes the language worth
using as a foundation at all: the mapping is total, unambiguous and cannot
surprise you.

Conveniences *above* that line — a `for` loop that expands into a counter and a
`repeat`, an expression syntax that compiles down to `operator_*` blocks, a
macro or templating layer — are not part of this compiler. They are a different
kind of tool: one that reads more convenient source and *emits raven-asm*, letting
this compiler stay small and exact underneath. Keeping the bottom layer boring is
what makes a layer above it possible to reason about, and it keeps this
repository's promise simple: the project you open in Scratch is the project you
wrote.

That tool now exists, and it is the other half of this repository:
[**raven**](/raven/) implements exactly the list above — `for`, `while`, `match`,
expression syntax, `f"…"` interpolation, types and a macro system — and compiles
to raven-asm. It follows the same discipline from the other side: every one of
those conveniences is a macro, and `raven expand` prints the raven-asm it became.
If you want the sugar but not the surprise, that is where it lives.

## Practical rules that follow

1. **No implicit conversions.** If a slot is hexagonal, hand it a boolean.
2. **No implicit declarations.** Variables, lists, broadcasts and procedures
   must be declared before use.
3. **No dead code.** A statement after a cap block is an error, not a warning.
4. **No silent mismatch.** An unknown dropdown value, costume name or sprite
   name is an error with the available values listed.
5. **No hidden generation.** Nothing is inlined, expanded or expanded-then-inlined.
   The only copy raven-asm makes is a `use`d module's procedures, and it tells you
   so in the [multi-file guide](/raven-asm/multi-file).
6. **Scope is declared, not inferred.** A variable belongs to the target whose
   file declares it, or to the stage when it is written `global`. A module has no
   target of its own, so a declaration there *must* be `global` — which means one
   module can never bind the same name to two different variables for two
   different users.
