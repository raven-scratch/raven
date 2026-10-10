# What the optimiser does

**It is a program, not a compiler pass.** `raven-asm build` emits one block per
statement, always, because that is the promise the language is built on; it has no
optimiser and no flag that turns one on. The folding lives in its own crate,
`raven-opt`, whose whole interface is *a raven-asm project in, an optimised
raven-asm project out*:

```sh
raven-opt --manifest-path raven-asm.toml --output optimised/   # a copy
raven-opt --manifest-path raven-asm.toml --in-place            # over the project
```

What comes out is raven-asm source, so `raven-asm build` accepts it and the
editor shows it, and every statement still maps to one block. Running the
optimiser is your decision because it is a separate run over a separate input —
and because the second run is a project like the first, you can diff the two,
build both and compare, or keep the plain one.

`raven` runs the same crate for you before it compiles, because its macros are
already lowerings a reader did not write; `raven build --no-optimize` leaves the
one-statement-one-block spelling in place there.

This page says what it rewrites, what it refuses to, and how you can tell that it
did not change your program.

## Why it is a separate program here and a step inside raven

The two front ends make opposite promises and the shape of the tooling follows
the promise:

| | raven-asm | raven |
| --- | --- | --- |
| Promise | what you write is what the editor shows | every convenience is a documented lowering |
| So the optimiser is | a program you run on the project | a step the build takes, on unless `--no-optimize` |
| To see the blocks | nothing to do | `raven build --no-optimize` |

## The two rewrites

Both are **identities in Scratch's own semantics, applied to operands the
compiler already knows.** That phrasing is the whole safety argument: a rewrite
is allowed when it depends only on what a block *does*, never on what your
program *means*.

### Constant folding

A reporter whose arguments are all literals is replaced by the value it would
have produced.

```rasm
// before
data_setvariableto("t", operator_add(operator_multiply(2, 3), 4));

// after
data_setvariableto("t", 10);
```

The arithmetic follows the VM's own definitions, not a convenient approximation.
That matters more than it sounds, because Scratch's operators are JavaScript's
and Rust's are not:

| Expression | Scratch | Why |
| --- | --- | --- |
| `mod(-7, 3)` | `2` | Floored, not truncated. The VM computes `n % m` and adds `m` when the signs differ. |
| `mod(1, 0)` | `NaN` | Not an error and not zero. |
| `round(-2.5)` | `-2` | `Math.round` rounds half **up**; Rust's `round` rounds half away from zero. |
| `"1" = 1` | `true` | `=` casts: two numbers compare numerically, anything else as case-insensitive text. |
| `mathop("sin", 90)` | `1` | Degrees, not radians. |

A fold this table cannot answer is not made. That is why the table is short: an
operator whose semantics this page cannot state exactly is left alone.

### Branch simplification and merging

An `if` whose condition the compiler can see is a literal is replaced by the
branch that runs. Both forms are handled, because both choose on the same test:

```rasm
// before
control_if(operator_gt(2, 1)) {
    looks_say("yes");
}

// after
looks_say("yes");
```

```rasm
// before
control_if_else(operator_gt(2, 1)) {
    looks_say("yes");
} else {
    looks_say("no");
}

// after
looks_say("yes");
```

The `else` half is dropped in the second case because it could not have run, and
the condition goes with it because nothing reads it any more. When the *`else`* is
the taken branch it becomes the body: the same statement with its two halves
swapped.

One case keeps the `if` and still saves work: if the taken branch ends in a
**cap block** (`control_stop`, `control_delete_this_clone`), splicing it into the
statement's slot would put the statements that follow the `if` after a cap, which
the emitter refuses and the source never had. So the statement survives as an
`if` with a literal condition and only the untaken branch is dropped.

The condition has to be a literal *the compiler put there* — either `<true>` in
your source or a fold this pass just performed. A condition that merely looks
constant, or one that reads a variable, is a different question and the optimiser
does not answer it.

## The one that surprises people: a boolean is a block

Scratch has no boolean **literal**. A hexagonal input holds a boolean *block* and
nothing else, so there is nothing to fold `operator_lt(1, 2)` *into* that is not
itself a block. The smallest block that is always true is `operator_equals(1,
1)`, and that is what the optimiser emits:

```rasm
// before
control_if(operator_lt(1, 2)) { looks_say("yes"); }

// after
control_if(operator_equals(1, 1)) { looks_say("yes"); }
```

Two consequences worth knowing:

* Folding a boolean does **not** always save a block. It saves the operand tree
  (`operator_lt` plus its two numbers becomes `operator_equals` plus its two
  numbers) and it makes the branch simplification above possible, which is where
  the real saving is.
* Writing a bare `true` where a condition belongs is a compile error, and it will
  stay one. `expected a condition, found true` is the emitter telling you that
  Scratch's blocks do not work that way.

## What it will not do

| Not done | Because |
| --- | --- |
| Propagating a variable | `x` may be assigned between two reads, so replacing the second read with the first value changes the program. |
| Propagating a **raven local** | A `let`, a `for` counter and a procedure result are all cells of the shared `_vms` list, and the desktop example's `Machine` sprite has 1,144 cell reads against 976 cell writes — the cells are mutated, so a substituted value can be stale. |
| Folding `sensing_timer()`, `looks_size()`, `sensing_answer()` | These read the world. Two evaluations can differ, so there is no constant to fold to. |
| Duplicating any such reporter | The same reason, from the other side. |
| Folding a condition that only *looks* constant | The optimiser acts on literals, not on inferences. |
| Inlining a procedure into its call sites | Measured, and it does not pay. See below. |

## Procedure inlining and reporter substitution: tried, measured, removed

### Inlining a procedure into its call sites

This is the obvious candidate — a Scratch custom-block call is not free — so it
was written and measured before being left out. Three findings, each of which
alone is enough:

1. **It does not pay on this repository's own projects.** With the full guard set
   (one-statement body, each parameter read at most once, pure arguments, a
   compatible `warp` setting, and a strict check that the body is cheaper than
   the call plus its arguments) it inlined **zero** calls across `desktop`,
   `chess`, `sudoku`, `penfont` and `case`. Without the cheaper-than-the-call
   check it inlined four calls in `desktop` and *added* 69 blocks.
2. **The dynamic cost is not where the static cost is.** `cpu_translate` has 37
   static call sites and 16.5 million dynamic ones; duplicating its body 37 times
   changes nothing about the loop that actually pays for it.
3. **A `warp` procedure cannot be inlined into a yielding caller.** `warp` means
   the runtime will not interrupt the thread; a copy runs under the caller's
   schedule and *can* be interrupted. That is observable, so the guard is
   required — and it rules out most procedures in a program like the desktop
   example, which is `warp` throughout.

### Substituting a reporter into the place that reads it

The other obvious one, and here it is **impossible rather than merely
unprofitable**. A raven local is not a value: `crates/raven` lowers every `let`,
`for` counter and procedure result into one project-wide list, `_vms`, addressed
by a compile-time constant index, so a read of a local is a `data_itemoflist` of a
cell. In the desktop example's `Machine` sprite there are **1,144 cell reads
against 976 cell writes** — the cells are mutated. Replacing a read with the value
an earlier write computed would propagate a value across a later write, which is
exactly the variable propagation this page refuses on principle.

The one case that looks safe is a cell written and read once with nothing in
between, and recognising it needs dataflow over the whole arena: every
`control_repeat`, every `procedures_call` and every `control_stop` is a place the
analysis has to be right about. That is a far larger and riskier pass than
everything else here, for a saving nobody has shown to exist.

The saving is real, though, and the place to take it is the source rather than the
optimiser: a reporter computed where it is used is cheaper than one stored and read
back, because a stored value costs a cell read or a call frame on top of the
computation itself. In raven that is what an `fn`, a `macro` or a `const` buys —
see [what a call costs](/raven/lowering#what-a-call-costs-and-why-an-inlined-reporter-is-cheaper).

## How you can tell it did not change your program

Three checks, each answering a different question:

* **`crates/raven-re/tests/roundtrip.rs`** compiles a project, reverses it with
  raven-re, compiles it again, and compares the two through a fingerprint of
  everything Scratch can observe. It runs with the optimiser on *and* off, so a
  rewrite that changed the emitted blocks fails the build.
* **The unit tests in `crates/raven-opt/src/optimize.rs`** pin the arithmetic
  against the VM's definitions (the table above is those tests) and, just as
  importantly, assert the rewrites that must *not* happen.
* **`examples/raven/desktop/tools/bench-optimize.mjs`** builds a project both
  ways, runs both in the same
  process on the same host, and prints the guest instruction count each retired.
  Identical instruction counts mean the two builds are running the same program;
  the seconds show the cost.

## What it is worth

On this repository's own examples (`node tools/bench-optimize-blocks.mjs`):

| Example | Blocks, plain | Blocks, optimised | Δ |
| --- | --- | --- | --- |
| `examples/raven/desktop` | 12,075 | 11,544 | −531 |
| `examples/raven/chess` | 31,158 | 31,043 | −115 |
| `examples/raven/sudoku` | 2,027 | 2,013 | −14 |
| `examples/raven/penfont` | 891 | 887 | −4 |
| `examples/raven/case` | 170 | 170 | 0 |
| **total** | **46,321** | **45,657** | **−664 (−1.43%)** |

Modest, and worth being honest about: a boot of the desktop example retires the
same 113,685,077 guest instructions either way and the wall clock differs by less
than the host's own run-to-run noise. What the layer buys is fewer blocks and the
guarantee that it cannot have changed anything — not a faster boot. The wins that
*are* large in that project came from removing procedure calls at the source,
which is a decision a compiler cannot make for you.
