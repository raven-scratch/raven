# raven-opt

A raven-asm project in, an optimised raven-asm project out.

```sh
raven-opt --manifest-path raven-asm.toml --output optimised/
raven-opt --manifest-path raven-asm.toml --in-place
```

What comes out is raven-asm source that builds with `raven-asm build` and means
what it meant before, with fewer Scratch blocks in it.

## Why it is a separate program

raven-asm promises that **one statement is one Scratch block**: what you write is
what the editor shows, and a reader can count the blocks. A compiler that folded
behind the reader's back would break that, so `raven-asm` has no way to ask for
this at all — the optimiser is a second command over a second input, and running
it is a decision the reader makes by running it.

`raven` runs it for you, because raven's macros are already lowerings a reader
never wrote. `raven build --no-optimize` is the escape.

## What it rewrites

Both rewrites are **identities in Scratch's own semantics, applied to operands
the compiler already knows** — which is what makes them safe to make without
reading the program.

* **Constant folding.** A reporter whose arguments are all literals becomes the
  value it would have produced. The arithmetic follows the VM's own definitions
  and not a convenient approximation: `mod(-7, 3)` is `2` because Scratch's
  remainder is floored, `round(-2.5)` is `-2` because `Math.round` rounds half
  *up*, and `"1" = 1` is true because `=` casts before it compares.
* **Branch simplification and merging.** An `if` or `if`/`else` whose condition
  the compiler can see runs exactly one branch, so only that branch is emitted.
  If the taken branch ends in a cap block the `if` stays, with only the untaken
  branch dropped.

A boolean is spelled as `operator_equals` between two literals, because Scratch
has no boolean literal for a hexagonal input to hold. So folding a condition does
not always save a block — the branch it makes dead is where the saving is.

## What it will not do

| Not done | Because |
| --- | --- |
| Propagating a variable | `x` may be assigned between two reads. |
| Propagating a raven local | A `let`, a `for` counter and a procedure result are cells of the shared `_vms` list, and the desktop example has 1,144 cell reads against 976 cell writes. |
| Folding `sensing_timer()`, `looks_size()` | They read the world; two evaluations can differ. |
| Folding a condition that only *looks* constant | The optimiser acts on literals, not on inferences. |
| Inlining a procedure | Measured, and it does not pay — it inlined zero calls across this repository's examples with a full guard set, and added 69 blocks without one. |

## How you can tell it did not change your program

* `crates/raven-re/tests/roundtrip.rs` compiles a project, reverses it, compiles
  it again, and compares the two through a fingerprint of everything Scratch can
  observe — with the optimiser on **and** off.
* The unit tests in `src/optimize.rs` pin the arithmetic against the VM's
  definitions and assert the rewrites that must *not* happen.
* `tools/bench-optimize-blocks.mjs` prints the block count of every example both
  ways, and `examples/raven/desktop/tools/bench-optimize.mjs` runs both builds in
  one process and compares the guest instructions each retires — identical counts
  mean the two builds are the same program.

## What it is worth

Across this repository's own examples it removes 664 blocks of 46,321 — **1.43%**.
That is modest, and worth being honest about: a boot of the desktop example
retires the same 113,685,077 guest instructions either way. What the layer buys is
fewer blocks and the guarantee that it cannot have changed anything, not a faster
boot. The large wins in that project came from removing procedure calls at the
source, which is a decision a compiler cannot make for you.
