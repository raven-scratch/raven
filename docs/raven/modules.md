# Modules

A module is a file. There are no inline `mod` blocks, no `mod.rs`, and no path
attributes: the path *is* the file path, relative to `src/`, with `/` written as
`::`.

```
src/
├── stage.rav                  the stage target
├── sprites/
│   ├── player.rav             a sprite target
│   └── enemy.rav              another
└── lib/
    ├── geometry.rav           module `lib::geometry`
    └── text/
        └── wrap.rav           module `lib::text::wrap`
```

## Two kinds of file

A `.rav` file declares a target or it does not. The distinction is the same one
raven-asm makes, and for the same reason.

**A target file** declares exactly one `stage` or `sprite` and holds its code.

```rav
// src/sprites/player.rav
sprite "Player" {
    costume "idle" = "assets/idle.svg";
    on flag_clicked { }
}
```

**A module file** declares no target. It holds macros, `fn`s, `const`s, `proc`s,
and project-wide state, and other files import it.

```rav
// src/lib/geometry.rav
pub const TAU: num = 6.283185307179586;

pub fn hypot(a: num, b: num) -> num {
    operators::mathop(MathOp::Sqrt, a * a + b * b)
}

pub proc square(size: num) warp {
    repeat 4 {
        motion::move_steps(size);
        motion::turn_right(90);
    }
}
```

A module has no target, so it cannot give one a costume, a sound or a
sprite-local variable. A `pub var` is the state it *can* declare, because
project-wide state belongs to the stage and there is only one stage. A costume, a
sound or a non-`pub` `var` written in a module file is accepted and then ignored —
nothing in the project changes — so those belong in the target file.

## `use`

```text
use       = "use" path [ "::" "{" ident { "," ident } "}" ] ";"
```

```rav
use lib::geometry;                       // the module
use lib::geometry::hypot;                // one item from it
use lib::geometry::{hypot, square};      // two items from it
use std::pen;                            // an intrinsic module needs no import
```

A path names a module, or a module and one item in it, and the two are told
apart by looking: `src/lib/geometry.rav` exists, `src/lib/geometry/hypot.rav`
does not, so `lib::geometry::hypot` must be the module `lib::geometry` and the
item `hypot`.

Paths are resolved from `src/`. Because Scratch has no per-target linkage for
compile-time items, importing a module puts **all** of its public items in scope
by name; naming items — `use lib::geometry::{hypot, square}` — does not restrict
that, it documents it, and the compiler checks that each name exists and is
`pub`. There is no `geometry::hypot` qualifier: a name is either in scope or it
is not.

The prelude is imported into every file without being written.

## What `use` costs

This is the part that is worth reading twice, because it is where raven is honest
about Scratch rather than convenient.

| Imported item | What the importer gets | What it costs |
| --- | --- | --- |
| `macro` | the definition, inlined at each call site | nothing, ever |
| `fn` | the definition, inlined at each call site | nothing, ever |
| `const` | the literal | nothing, ever |
| `proc` | **a copy of the custom block, in that target** | one custom block per importing target |
| `pub var` / `pub var list` | the shared stage variable | one variable, once, for the project |

A Scratch custom block belongs to exactly one target. There is no way to share one
between a stage and two sprites, so raven copies it — as raven-asm's `use` does,
and as the Scratch editor itself does when you duplicate a sprite. The difference
is that raven tells you, and `raven expand` shows you the copies.

Combined with the fact that macros and `fn`s are compile-time, this gives a clean
rule of thumb:

> **Put logic that should be shared and free in a `macro` or an `fn`. Put logic
> that must be a real custom block in a `proc`, and expect a copy per sprite.**

## Visibility

* `pub` — importable by other modules, and for a `var`, placed on the stage.
* no `pub` — visible only inside the declaring file.
* Importing a non-`pub` item is an error that names the module and the missing
  `pub`.
* `pub` is only meaningful on `var`, `list`, `const`, `fn`, `macro`, `proc` and
  `struct`. On anything else — a `watch`, a `broadcast`, a `costume`, a `sound` or
  an `on` script — it is rejected.
* A `pub struct` parses, but it cannot travel: a struct is resolved inside the
  target that declares it, so it is not usable through `use`.
* Every `use` comes before the first item in a file.

There is no `pub(crate)` and no nesting of visibility: a module is a file, and an
item is either shared or it is not.

## Cycles

Module imports must be acyclic. A cycle is reported with the full path of the
import chain rather than the first repeated name, because a cycle is only
diagnosable when you can see the loop.

Macro expansion has its own acyclicity requirement, and it is checked separately:
a module graph can be a DAG while its macros are not, and vice versa. Both errors
name what recurs.

## Project-wide state

A `pub var` declared anywhere is placed on the stage, so it has exactly one
instance, and every sprite reads and writes that one variable.

```rav
// src/lib/scoring.rav
pub var high_score: num = 0;

// src/sprites/player.rav
use lib::scoring::high_score;

sprite "Player" {
    on flag_clicked {
        if score > high_score { high_score = score; }
    }
}
```

Because the variable lives on the stage, there is no ambiguity about whose copy
you are reading — which is exactly raven-asm's rule, and the reason raven's
[law 8](/raven/design#_8-ownership-is-declared-never-inferred) insists that
ownership be written down rather than inferred from use.
